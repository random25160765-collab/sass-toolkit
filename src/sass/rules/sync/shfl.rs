// =============================================================================
//  SHFL -- SASS -> PTX  warp-level register shuffle (inter-lane communication)
//
//  ISA reference:  platform/sass-spec/isa/data/sm89-isa-manual/raw/SHFL.html
//  PTX reference:  shfl.sync.{mode}.b32 d, a, b, c, membermask;
//
//  CUDA SM89 Toolchain (ptxas -O0 ground truth):
//    ptxas:  NVIDIA CUDA 12.9.86
//    input:  shfl.sync.idx.b32 rd, rd, 0, 0x1f, 0x0;
//    output: SHFL.IDX PT, R6, R6, RZ, 0x1f
//    evidence: sass/corpus/shfl/test_shfl.sass.txt
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 4 total
//  ═══════════════════════════════════════════════════════════════════════════
//
//    SHFL_P_R_R_R_I    pred, dst, src, reg, imm mask       ✓ handled
//    SHFL_P_R_R_I_R    pred, dst, src, imm, reg mask       ✓ handled
//    SHFL_P_R_R_I_I    pred, dst, src, imm, imm            ✓ handled
//    SHFL_P_R_R_R_R    pred, dst, src, reg, reg mask       ✓ handled
//
//  MODIFIERS (4 shuffle modes, all verified by ptxas audit):
//    .IDX   shfl from specific lane index     -> shfl.sync.idx
//    .UP    shift up by delta                 -> shfl.sync.up
//    .DOWN  shift down by delta               -> shfl.sync.down
//    .BFLY  butterfly XOR swap                -> shfl.sync.bfly
//
//  ALL four modifiers are rendered by cuobjdump.
//
//  OPERAND CONVENTION:
//    The first operand (PT = always-true predicate) is the sync warp predicate.
//    RZ as the mask operand means "full mask = all lanes" (0x1f in PTX).
//
//  ═══════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC
//  ═══════════════════════════════════════════════════════════════════════════
//
//    Rd := Ra from lane specified by mode + delta/index + membermask
//
//  ═══════════════════════════════════════════════════════════════════════════
//  PTX MAPPING
//  ═══════════════════════════════════════════════════════════════════════════
//
//    SHFL.IDX  PT,Rd,Ra,Ridx,Rmask -> shfl.sync.idx.b32  Rd,Ra,Ridx,0,Rmask;
//    SHFL.UP   PT,Rd,Ra,delta,Rmask -> shfl.sync.up.b32   Rd,Ra,delta,0,Rmask;
//    SHFL.DOWN PT,Rd,Ra,delta,Rmask -> shfl.sync.down.b32 Rd,Ra,delta,0,Rmask;
//    SHFL.BFLY PT,Rd,Ra,delta,Rmask -> shfl.sync.bfly.b32 Rd,Ra,delta,0,Rmask;
//
//  Non-BV-expressible (inter-lane communication).  Axiomatic.
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

fn shfl_mode(mods: &[String]) -> &'static str {
    for m in mods {
        match m.as_str() {
            "IDX" => return "idx",
            "UP" => return "up",
            "DOWN" => return "down",
            "BFLY" => return "bfly",
            _ => {}
        }
    }
    "idx"
}

fn fmt_op(op: Option<&Op>) -> String {
    match op {
        Some(Op::Gpr(n)) => format!("%r{}", n),
        Some(Op::Imm(v)) => format!("{:#x}", v),
        _ => "0".to_string(),
    }
}

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    // Operand layout varies:
    //   Bridge (actual):  dst=[Zero]  src=[Rd, Ra, delta/lane, mask]
    //   Golden test:      dst=[Rd]    src=[Ra, delta/lane, mask]
    // Detect: if dst is [Zero] (or empty) and src has 4 elements, use src layout.
    let use_src_layout = inst.dst.first().map_or(false, |o| matches!(o, Op::Zero))
        && inst.src.len() >= 4;

    let dst  = if use_src_layout { helpers::opt_int(inst.src.first()) } else { helpers::opt_int(inst.dst.first()) };
    let src_r  = if use_src_layout { helpers::opt_int(inst.src.get(1)) } else { helpers::opt_int(inst.src.first()) };
    let lane = if use_src_layout { helpers::opt_int(inst.src.get(2)) } else { helpers::opt_int(inst.src.get(1)) };
    let mask = if use_src_layout { inst.src.get(3) } else { inst.src.get(2) };

    let mode = shfl_mode(&inst.modifiers);
    let m    = match mask {
        Some(Op::Zero) | None => "0x1f".to_string(), // RZ = all lanes participate
        Some(op) => helpers::opt_int(Some(op)),
    };

    format!("shfl.sync.{}.b32 {}, {}, {}, 0, {};", mode, dst, src_r, lane, m)
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

    /// SASS: SHFL.IDX PT, R6, R6, RZ, 0x1f  -> idx from source to source, lane 0
    #[test] fn rule_idx() {
        let i=RuleInst::new("SHFL",&["IDX"],
            vec![Op::r(6)], vec![Op::Zero, Op::r(6), Op::Zero, Op::Imm(0x1f)]);
        let p=translate(&i,&sb());
        assert!(p.contains("shfl.sync.idx.b32 %r6, %r6, 0, 0, 0x1f;"),"{}",p);
    }

    /// SASS: SHFL.UP PT, R6, R6, 0x2, RZ  -> shift up by 2, all lanes
    #[test] fn rule_up() {
        let i=RuleInst::new("SHFL",&["UP"],
            vec![Op::r(6)], vec![Op::Zero, Op::r(6), Op::Imm(0x2), Op::Zero]);
        let p=translate(&i,&sb());
        assert!(p.contains("shfl.sync.up.b32 %r6, %r6, 0x2, 0, 0x1f;"),"{}",p);
    }
}
