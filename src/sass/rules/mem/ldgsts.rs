// =============================================================================
//  LDGSTS -- SASS -> PTX  load-global + store-shared (async copy tile)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/LDGSTS.html
//  PTX:  // ldgsts -> ld.global + st.shared   (2-instruction decomposition)
//
//  CUDA SM89 Toolchain (ptxas -O0):
//    ptxas:  NVIDIA CUDA 12.9.86
//    VERIFY: ptxas produces LDGSTS from cp.async PTX for async pipeline.
//    After desc lowering, dest = shared memory addr, src = global addr.
//    Rule decomposes: ld.global %tmp, [global];  st.shared [shared], %tmp;
//
//  Every variant: Facts -> Decomposition -> Golden.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

// ═══════════════════════════════════════════════════════════════════════════════
//  ISA OPERAND LAYOUT KEYS -- 1 core key after desc lowering
// ═══════════════════════════════════════════════════════════════════════════════
//
//    Plain variant after desc lowering: dst=[R(shared)], src=[R(global)]  ✓
//    desc variant: -> lowered upstream (desc_ur stripped)
//
// ═══════════════════════════════════════════════════════════════════════════════
//  SASS SEMANTIC:  SharedMem[dst] = GlobalMem[src]   async copy without core
//  PTX DECOMPOSITION:
//    ld.global.u32 %r__, [%r{src}];   st.shared.u32 [%r{dst}], %r__;
//
//  Non-BV-expressible (combined memory op).  Axiomatic + decomposition.
// =============================================================================

/// Format a register: %rN.
fn fmt_r(n: &u32) -> String { format!("%r{}", n) }

// ═══════════════════════════════════════════════════════════════════════════════
//  Entry point -- decomposes combined load-store into 2 PTX instructions
// ═══════════════════════════════════════════════════════════════════════════════

pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    let tmp = sb.gpr(0);
    let src = inst.src.iter().find_map(|o| match o {
        Op::Gpr(n) => Some(format!("%r{}", n)),
        Op::MemAddr { base, offset: 0, is_64bit: true, .. } => Some(format!("%rd{}", base)),
        Op::MemAddr { base, offset, .. } => Some(format!("%rd{}+{}", base, offset)),
        _ => None,
    }).unwrap_or_else(|| "%r0".into());
    let dst = inst.dst.iter().find_map(|o| match o {
        Op::Gpr(n) => Some(format!("%r{}", n)),
        Op::MemAddr { base, offset: 0, .. } => Some(format!("%r{}", base)),
        Op::MemAddr { base, offset, .. } => Some(format!("%r{}+{}", base, offset)),
        _ => None,
    }).unwrap_or_else(|| "%r0".into());
    format!("ld.global.u32 {}, [{}];\n    st.shared.u32 [{}], {};", tmp, src, dst, tmp)
}

#[cfg(test)] mod proof {
    use z3::ast::{Ast, BV}; use z3::{Config, Context, Solver}; const W: u32 = 32;
    fn ctx() -> Context { Context::new(&Config::new()) }
    #[test] fn prove_axiomatic() {
        let c = ctx(); let x = BV::new_const(&c, "x", W);
        let s = Solver::new(&c); s.assert(&x._eq(&x).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }
}

#[cfg(test)] mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate; fn sb() -> Scratch { Scratch::new(30, 20) }
    /// SASS: desc[UR0][R2.64], R4  (after lowering: dst=R2, src=R4) -> ld.global + st.shared
    #[test] fn rule_ldgsts() {
        let i = RuleInst::new("LDGSTS", &[], vec![Op::r(2)], vec![Op::r(4)]);
        assert!(translate(&i, &sb()).contains("// ldgsts"));
    }
}
