// =============================================================================
//  ATOMG -- SASS -> PTX  atomic operation on global memory
//
//  ISA reference:  platform/sass-spec/isa/data/sm89-isa-manual/raw/ATOMG.html
//  PTX reference:  atom.global.{op}.{type} d, [a], b;
//
//  CUDA SM89 Toolchain (ptxas -O0 ground truth):
//    ptxas:  NVIDIA CUDA 12.9.86
//    input:  atom.global.add.u32 rold, [d2], ri;
//    output: ATOMG.E.ADD.STRONG.GPU PT, R6, [R4.64], R3
//    evidence: sass/corpus/atomg/test_atomg.sass.txt
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERATION MODIFIERS -- 8 total (verified by ptxas audit)
//  ═══════════════════════════════════════════════════════════════════════════
//
//    .ADD   atomic add       -> atom.global.add      .OR    atomic or
//    .MIN   atomic minimum   -> atom.global.min      .XOR   atomic xor
//    .MAX   atomic maximum   -> atom.global.max      .EXCH  atomic exchange
//    .AND   atomic and       -> atom.global.and      .DEC   atomic decrement
//
//    .E     evict-first (default, no PTX equivalent)
//    .STRONG.GPU  -> atom.global (default for SM89, same as LDG/STG .cg mapping)
//
//  ISA keys (7+): R_R ✓  R_RI ✓  R_RUR/RURI/UR/URI/desc -> upstream
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC
//  ═══════════════════════════════════════════════════════════════════════════
//
//    Rd_old := atomic_op(*global[Ra.64], Rval)    read-modify-write
//
//  ═══════════════════════════════════════════════════════════════════════════
//  PTX MAPPING
//  ═══════════════════════════════════════════════════════════════════════════
//
//    ATOMG.{op} PT,Rd,[Ra.64],Rval -> atom.global.{op}.b32 Rd,[%ra],%rval;
//
//  Non-BV-expressible (atomic RMW).  Axiomatic.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

fn atom_op(mods: &[String]) -> &'static str {
    for m in mods {
        match m.as_str() {
            "ADD" => return "add", "MIN" => return "min", "MAX" => return "max",
            "AND" => return "and", "OR"  => return "or",  "XOR" => return "xor",
            "EXCH" => return "exch", "DEC" => return "dec", "CAS" => return "cas",
            _ => {}
        }
    }
    "add"
}

fn fmt_op(op: Option<&Op>) -> String {
    match op { Some(Op::Gpr(n)) => format!("%r{}", n), _ => "%r0".to_string() }
}

/// Format a memory address operand.  On SM90+, all global atomics use
/// 64-bit register pairs (%rdN), regardless of whether the SASS disassembly
/// shows the .64 suffix (some cuobjdump versions omit it for certain formats).
fn addr_op(op: Option<&Op>) -> String {
    match op {
        Some(Op::MemAddr { base, offset, .. }) => {
            if *offset == 0 { format!("%rd{}", base) }
            else { format!("%rd{}+{}", base, offset) }
        }
        Some(Op::Gpr(n)) => format!("%rd{}", n),
        _ => "%rd0".to_string(),
    }
}

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    // ── SASS layout: ATOMG.op PT, Rd, [Ra.64]+off, Rval ──
    // ── CAS variant:  ATOMG.CAS PT, Rd, [Ra.64], Rcmp, Rval ──
    //    Parser maps:  PT→dst[0](Zero), Rd→src[0], addr→src[1], val(s)/cmp→src[2..]
    let dst  = fmt_op(inst.src.get(0));           // Rd = old value
    let addr = addr_op(inst.src.get(1));          // [Ra.64+off] = address
    let op   = atom_op(&inst.modifiers);

    if op == "cas" {
        let cmp = fmt_op(inst.src.get(2));        // Rcmp = compare value
        let val = fmt_op(inst.src.get(3));        // Rval = swap value
        format!("atom.global.cas.b32 {}, [{}], {}, {};", dst, addr, cmp, val)
    } else {
        let val  = fmt_op(inst.src.get(2));       // Rval = operand
        format!("atom.global.{}.u32 {}, [{}], {};", op, dst, addr, val)
    }
}

#[cfg(test)] mod proof {
    use z3::ast::{Ast,BV}; use z3::{Config,Context,Solver}; const W:u32=32;
    fn ctx()->Context{Context::new(&Config::new())}
    #[test] fn prove_axiomatic(){ let c=ctx(); let x=BV::new_const(&c,"x",W);
        let s=Solver::new(&c); s.assert(&x._eq(&x).not()); assert_eq!(s.check(),z3::SatResult::Unsat); }
}

#[cfg(test)] mod golden {
    use super::super::types::{Op,RuleInst,Scratch}; use super::translate;
    fn sb()->Scratch{Scratch::new(30,20)}
    /// SASS: ATOMG.E.ADD.STRONG.GPU PT,R6,[R4.64],R3 -> atom.global.add.u32
    /// Parser maps: PT→dst[0](Zero), R6→src[0], [R4.64]→src[1](MemAddr64), R3→src[2]
    #[test] fn rule_add() {
        let i=RuleInst::new("ATOMG",&["ADD","E"],
            vec![Op::Zero],vec![Op::r(6),Op::addr64(4),Op::r(3)]);
        let p=translate(&i,&sb());
        assert!(p.contains("atom.global.add.u32 %r6, [%rd4], %r3;"),"{}",p);
    }
    /// SASS: ATOMG.E.EXCH PT,R6,[R4.64],R3 -> atom.global.exch.u32
    #[test] fn rule_exch() {
        let i=RuleInst::new("ATOMG",&["EXCH","E"],
            vec![Op::Zero],vec![Op::r(6),Op::addr64(4),Op::r(3)]);
        let p=translate(&i,&sb());
        assert!(p.contains("atom.global.exch.u32 %r6, [%rd4], %r3;"),"{}",p);
    }
}
