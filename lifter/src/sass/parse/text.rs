//! SASS disassembly parser for cuobjdump text output.
//!
//! All SASS parsing goes through `cuobjdump -sass` output.
//! The binary decode path (SassDisassembler, OpcodeTable, SmVersion) has been
//! removed — it was incomplete (no cNEG/cABS/cache policy decoding) and misleading.

use std::collections::HashMap;
use std::fmt;

use super::instruction::*;

// ============================================================================
// Text-based Disassembly Parser (for cuobjdump output)
// ============================================================================

/// Extract hex-encoded instruction bits from a trailing `/* 0x... */` comment.
fn extract_hex_encoding(after_addr: &str) -> u64 {
    if let Some(hex_start) = after_addr.rfind("/* 0x") {
        let after = after_addr[hex_start..].trim_start_matches('/').trim_start_matches('*').trim();
        let hex_part = after.trim_start_matches("0x").trim_start_matches("0X");
        let end = hex_part.find("*/").unwrap_or(hex_part.len());
        if let Ok(v) = u64::from_str_radix(&hex_part[..end].trim(), 16) { return v; }
    }
    0
}

/// Parser for cuobjdump-style text output
pub struct TextDisassemblyParser;

impl TextDisassemblyParser {
    /// Parse a single instruction line from cuobjdump output
    pub fn parse_instruction_line(line: &str) -> Option<EnhancedSassInstruction> {
        let line = line.trim();

        // Format: /*0050*/ @P0 LDG.E.U32 R0, [R2.64+0x10] ; /* 0x... */

        // Extract address from /*xxxx*/
        if !line.starts_with("/*") {
            return None;
        }

        let addr_end = line.find("*/")?;
        let addr_str = line.get(2..addr_end)?.trim();
        let address = u64::from_str_radix(addr_str, 16).ok()?;

        // Extract hex encoding from trailing /* 0x... */ comment
        let encoding_lo = extract_hex_encoding(line.get(addr_end + 2..).unwrap_or(""));
        let encoding_hi: Option<u64> = None; // filled by parse_cuobjdump_output second pass

        // Get rest of line after address
        let rest = strip_cuobjdump_trailing_metadata(line.get(addr_end + 2..)?.trim()).trim();

        // Check for predicate (@P0, @!P1, etc.)
        let (predicate, instruction_part) = if rest.starts_with('@') {
            let space_idx = rest.find(' ').unwrap_or(rest.len());
            let pred_str = rest.get(..space_idx)?;
            let pred_op = SassOperand::parse(pred_str)?;
            (Some(pred_op), rest.get(space_idx..)?.trim())
        } else {
            (None, rest)
        };

        // Split into opcode+modifiers and operands
        let parts: Vec<&str> = instruction_part.split_whitespace().collect();
        if parts.is_empty() {
            return None;
        }

        let opcode_full = parts[0];
        // Split opcode from modifiers (e.g., "LDG.E.U32" -> "LDG", [".E", ".U32"])
        let opcode_parts: Vec<&str> = opcode_full.split('.').collect();
        let opcode = opcode_parts[0].to_string();
        let modifiers: Vec<String> = opcode_parts[1..].iter().map(|s| s.to_string()).collect();

        // Parse data type from modifiers
        let data_type = modifiers
            .iter()
            .find_map(|m| SassDataType::from_modifier(m));

        // Parse operands
        let operands_str = parts[1..].join(" ");
        let operand_strs = split_top_level_operands(&operands_str);

        // First operand is typically destination
        let dest_operands: Vec<SassOperand> = if !operand_strs.is_empty() {
            SassOperand::parse(operand_strs[0]).into_iter().collect()
        } else {
            Vec::new()
        };

        // Rest are source operands
        let src_operands: Vec<SassOperand> = operand_strs
            .iter()
            .skip(1)
            .filter_map(|s| SassOperand::parse(s))
            .collect();

        // Determine opcode class
        let opcode_class = classify_opcode(&opcode);
        let memory_space = get_memory_space(&opcode);

        // Infer vector width from bit-width modifiers: .64 → 2, .128 → 4, .256 → 8
        let vector_width = modifiers.iter()
            .find_map(|m| m.parse::<u32>().ok())
            .map(|bits| (bits / 32) as u8)
            .unwrap_or(1);

        Some(EnhancedSassInstruction {
            opcode: opcode.clone(),
            // ★ store CLEAN text: address prefix + trailing metadata already stripped by parser
            instruction_text: instruction_part.to_string(),
            address,
            size: 16, // Assume 128-bit
            encoding_lo,
            encoding_hi,
            opcode_class,
            memory_space,
            data_type,
            vector_width,
            predicate,
            dest_operands,
            src_operands,
            modifiers,
            control_codes: SassControlCodes::default(),
            ptx_template: get_ptx_template(&opcode),
            ptx_equivalent: None,
            ptx_file: None,
            ptx_line: None,
            ptx_column: None,
            function_name: None,
            data_dependencies: Vec::new(),
            basic_block_id: None,
        })
    }

    /// Parse full cuobjdump -sass -lineinfo output
    pub fn parse_cuobjdump_output(output: &str) -> Vec<EnhancedSassInstruction> {
        let mut instructions = Vec::new();
        let mut current_file = String::from("kernel.ptx");
        let mut current_line = 0u32;
        let mut current_function = None;

        for line in output.lines() {
            let line = line.trim();

            // Parse function header: "Function : kernel_name"
            if line.starts_with("Function :") || line.starts_with("function :") {
                if let Some(name) = line.split(':').nth(1) {
                    current_function = Some(name.trim().to_string());
                }
                continue;
            }

            // Parse file reference: ## File "kernel.ptx", line 16
            if line.contains("## File") {
                if let Some(file_part) = line.split('"').nth(1) {
                    current_file = file_part.to_string();
                }
                if let Some(line_part) = line.split("line ").nth(1) {
                    if let Ok(line_num) = line_part
                        .trim_end_matches(|c: char| !c.is_ascii_digit())
                        .parse::<u32>()
                    {
                        current_line = line_num;
                    }
                }
                continue;
            }

            // Parse line marker: ## Line 16
            if let Some(line_marker) = line.strip_prefix("## Line ") {
                if let Ok(line_num) = line_marker.trim().parse::<u32>() {
                    current_line = line_num;
                }
                continue;
            }

            // Parse instruction
            if line.starts_with("/*") {
                if let Some(mut inst) = Self::parse_instruction_line(line) {
                    inst.ptx_file = Some(current_file.clone());
                    inst.ptx_line = Some(current_line);
                    inst.function_name = current_function.clone();
                    instructions.push(inst);
                } else {
                    // Hex-only continuation line: "/* 0x008fc80007ffe0ff */"
                    // Attach to previous instruction's encoding_hi.
                    let hex = extract_hex_encoding(line);
                    if hex != 0 {
                        if let Some(last) = instructions.last_mut() {
                            last.encoding_hi = Some(hex);
                        }
                    }
                }
            }
        }

        instructions
    }
}

fn strip_cuobjdump_trailing_metadata(line: &str) -> &str {
    let mut bracket_depth = 0u32;

    for (idx, ch) in line.char_indices() {
        match ch {
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            ';' if bracket_depth == 0 => return line[..idx].trim_end(),
            '/' if bracket_depth == 0 && line[idx..].starts_with("/*") => {
                return line[..idx].trim_end();
            }
            '&' | '?' if bracket_depth == 0 && at_token_boundary(line, idx) => {
                return line[..idx].trim_end();
            }
            _ => {}
        }
    }

    line.trim_end()
}

fn at_token_boundary(line: &str, idx: usize) -> bool {
    idx == 0
        || line[..idx]
            .chars()
            .next_back()
            .map(char::is_whitespace)
            .unwrap_or(false)
}

fn split_top_level_operands(operands: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut bracket_depth = 0u32;

    for (idx, ch) in operands.char_indices() {
        match ch {
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            ',' if bracket_depth == 0 => {
                let operand = operands[start..idx].trim();
                if !operand.is_empty() {
                    out.push(operand);
                }
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }

    let operand = operands[start..].trim();
    if !operand.is_empty() {
        out.push(operand);
    }
    out
}

// ============================================================================
// Error Types
// ============================================================================

#[derive(Debug)]
pub enum DisassemblerError {
    ParseError(String),
}

impl fmt::Display for DisassemblerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DisassemblerError::ParseError(msg) => write!(f, "Parse error: {}", msg),
        }
    }
}

impl std::error::Error for DisassemblerError {}

// ============================================================================
// Control Flow Analysis
// ============================================================================

/// Control flow analyzer for SASS code
pub struct ControlFlowAnalyzer;

impl ControlFlowAnalyzer {
    /// Identify basic blocks in a sequence of instructions
    pub fn find_basic_blocks(instructions: &mut [EnhancedSassInstruction]) {
        if instructions.is_empty() {
            return;
        }

        // Build set of branch targets
        let mut branch_targets: std::collections::HashSet<u64> = std::collections::HashSet::new();

        for inst in instructions.iter() {
            if inst.opcode_class.is_control_flow() {
                for op in &inst.src_operands {
                    if let SassOperand::Address(addr) = op {
                        branch_targets.insert(*addr);
                    }
                }
            }
        }

        // Assign basic block IDs
        let mut current_block = 0u32;
        let mut new_block_next = true;

        for inst in instructions.iter_mut() {
            if new_block_next || branch_targets.contains(&inst.address) {
                current_block += 1;
                new_block_next = false;
            }

            inst.basic_block_id = Some(current_block);

            if inst.opcode_class.is_control_flow() {
                new_block_next = true;
            }
        }
    }

    /// Compute data dependencies between instructions
    pub fn analyze_data_flow(instructions: &mut [EnhancedSassInstruction]) {
        let mut last_writer: HashMap<String, u64> = HashMap::new();

        for inst in instructions.iter_mut() {
            for src in &inst.src_operands {
                if let SassOperand::Register(reg) = src {
                    let key = format!("{}{}", reg.prefix, reg.number);
                    if let Some(&writer_addr) = last_writer.get(&key) {
                        inst.data_dependencies.push(writer_addr);
                    }
                }
            }

            for dest in &inst.dest_operands {
                if let SassOperand::Register(reg) = dest {
                    if !reg.is_zero {
                        let key = format!("{}{}", reg.prefix, reg.number);
                        last_writer.insert(key, inst.address);
                    }
                }
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_parse_instruction() {
        let line = "/*0050*/ LDG.E.U32 R0, [R2.64] ;";
        let inst = TextDisassemblyParser::parse_instruction_line(line).unwrap();

        assert_eq!(inst.address, 0x50);
        assert_eq!(inst.opcode, "LDG");
        assert!(inst.modifiers.contains(&"E".to_string()));
        assert!(inst.modifiers.contains(&"U32".to_string()));
        assert_eq!(inst.opcode_class, SassOpcodeClass::GlobalLoad);
    }

    #[test]
    fn test_text_parse_with_predicate() {
        let line = "/*0100*/ @P0 BRA 0x200 ;";
        let inst = TextDisassemblyParser::parse_instruction_line(line).unwrap();

        assert_eq!(inst.address, 0x100);
        assert_eq!(inst.opcode, "BRA");
        assert!(inst.predicate.is_some());
    }

    #[test]
    fn test_text_parse_real_sm120_cuobjdump_annotations() {
        let line = "/*00b0*/                   IMAD.WIDE.U32 R2, R7, 0x4, R2              &req={0}         ?WAIT6_END_GROUP;  /* 0x0000000407027825 */";
        let inst = TextDisassemblyParser::parse_instruction_line(line).unwrap();

        assert_eq!(inst.address, 0x00b0);
        assert_eq!(inst.opcode, "IMAD");
        assert!(inst.modifiers.contains(&"WIDE".to_string()));
        assert!(inst.modifiers.contains(&"U32".to_string()));
        assert_eq!(inst.dest_operands.len(), 1);
        assert_eq!(inst.src_operands.len(), 3);
        assert_eq!(inst.src_operands[1], SassOperand::Immediate(0x4));
        assert_eq!(
            inst.src_operands[2],
            SassOperand::Register(SassRegister::new("R", 2))
        );
    }

    #[test]
    fn test_cuobjdump_parse() {
        let output = r#"
Function : test_kernel
## File "kernel.ptx", line 10
/*0000*/ MOV R1, c[0x0][0x20] ;
## Line 12
/*0010*/ LDG.E.U64 R2, [R4] ;
/*0020*/ FADD R0, R1, R2 ;
"#;

        let instructions = TextDisassemblyParser::parse_cuobjdump_output(output);
        assert_eq!(instructions.len(), 3);

        assert_eq!(instructions[0].ptx_line, Some(10));
        assert_eq!(instructions[1].ptx_line, Some(12));
        assert_eq!(instructions[2].ptx_line, Some(12));

        assert_eq!(
            instructions[0].function_name,
            Some("test_kernel".to_string())
        );
    }

    #[test]
    fn test_basic_block_detection() {
        let mut instructions = vec![
            EnhancedSassInstruction::new("MOV".to_string(), 0x00),
            EnhancedSassInstruction::new("ADD".to_string(), 0x10),
            EnhancedSassInstruction::new("BRA".to_string(), 0x20),
            EnhancedSassInstruction::new("MOV".to_string(), 0x30),
        ];

        ControlFlowAnalyzer::find_basic_blocks(&mut instructions);

        assert_eq!(instructions[0].basic_block_id, Some(1));
        assert_eq!(instructions[1].basic_block_id, Some(1));
        assert_eq!(instructions[2].basic_block_id, Some(1));
        assert_eq!(instructions[3].basic_block_id, Some(2)); // New block after BRA
    }
}
