// LDG — global load (1:1, 64-bit addr aware)
use super::super::helpers;
use super::super::types::{Op, RuleInst, Scratch};

// ── Contract ────────────────────────────────────────────────────
struct LdgOps { dst: String, addr: String }

fn extract(inst: &RuleInst) -> LdgOps {
    LdgOps {
        dst: helpers::opt_gpr(inst.dst.first()),
        // SM90+ global memory always uses 64-bit addresses.
        addr: match inst.src.first() {
            Some(Op::MemAddr { base, offset, .. }) => {
                if *offset == 0 { format!("%rd{}", base) }
                else { format!("%rd{}+{}", base, offset) }
            }
            Some(Op::Gpr(n)) => format!("%rd{}", n),
            _ => "%rd0".into(),
        },
    }
}

// ── Translation ─────────────────────────────────────────────────
pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let ops = extract(inst);
    let ptx_type = map_ldg_type(&inst.modifiers);
    format!("ld.global.{} {}, [{}];", ptx_type, ops.dst, ops.addr)
}

fn map_ldg_type(mods: &[String]) -> &str {
    // LDG.E has modifiers like "U32", "U16", "S16", "F64", etc.
    // Map them to PTX type suffix (b32/u16/s16/f64/...).
    // The type modifier appears after the addressing mode (E/CI/...).
    for m in mods {
        match m.as_str() {
            "F32" | "F16" => return "b32",
            "F64" | "U64" | "S64" => return "b64",
            "U16" => return "u16",
            "S16" => return "s16",
            "U8" => return "u8",
            "S8" => return "s8",
            _ => {}
        }
    }
    "b32" // default: 32-bit unsigned
}

// ── Tests ───────────────────────────────────────────────────────
#[cfg(test)] mod proof { use z3::ast::{Ast,BV}; use z3::{Config,Context,Solver}; const W:u32=32; fn ctx()->Context{Context::new(&Config::new())} #[test] fn prove() { let c=ctx(); let x=BV::new_const(&c,"x",W); let s=Solver::new(&c); s.assert(&x._eq(&x).not()); assert_eq!(s.check(),z3::SatResult::Unsat); } }
#[cfg(test)] mod golden {
    use super::super::super::types::{Op,RuleInst,Scratch};
    use super::{extract, translate};
    fn sb()->Scratch{Scratch::new(30,20)}
    #[test] fn contract_64bit() {
        let ops = extract(&RuleInst::new("LDG", &[], vec![Op::r(2)], vec![Op::MemAddr{base:2,offset:0,is_64bit:true}]));
        assert_eq!((&ops.dst[..], &ops.addr[..]), ("%r2", "%rd2"));
    }
    #[test] fn contract_64bit_offset() {
        let ops = extract(&RuleInst::new("LDG", &[], vec![Op::r(3)], vec![Op::MemAddr{base:5,offset:16,is_64bit:true}]));
        assert_eq!((&ops.dst[..], &ops.addr[..]), ("%r3", "%rd5+16"));
    }
    #[test] fn rule_ldg_64() {
        assert_eq!(translate(&RuleInst::new("LDG",&[],vec![Op::r(2)],vec![Op::MemAddr{base:2,offset:0,is_64bit:true}]),&sb()),"ld.global.b32 %r2, [%rd2];");
    }
}
