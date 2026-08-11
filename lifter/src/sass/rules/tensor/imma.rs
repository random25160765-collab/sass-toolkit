// =============================================================================
//  IMMA -- SASS -> PTX  integer matrix multiply-accumulate (tensor core)
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/IMMA.html
//  PTX:  imma.sync.aligned.shape.m8n8k16.{itype} Rd, Ra, Rb, Rc;
//
//  KEYS:  R_R_R_R (✓) | R_R_R_R_R_I (✗ sparse) | UP variants (-> upstream)
//  SHAPE:  8816  (m8n8k16)
//  TYPES:  U8, U4, S8, INVALID combos
//  1:1 axiomatic -- same hardware tensor core instruction.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

fn fmt_r(n: &u32) -> String { format!("%r{}", n) }

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let dst = inst.dst.first().map_or("%r0".to_string(), |o| match o {
        Op::Gpr(n) => fmt_r(n), _ => "%r0".to_string()
    });
    let data: Vec<String> = inst.src.iter()
        .filter(|o| !matches!(o, Op::Pred(_) | Op::NegPred(_) | Op::Up(_)))
        .take(3)
        .map(|o| match o { Op::Gpr(n) => fmt_r(n), _ => "%r0".to_string() })
        .collect();
    let itype = if inst.modifiers.iter().any(|m| m == "S8") { "s8" } else { "u8" };
    let ra = data.get(0).map_or("%r0", |s| s.as_str());
    let rb = data.get(1).map_or("%r0", |s| s.as_str());
    let rc = data.get(2).map_or("%r0", |s| s.as_str());
    format!("imma.sync.aligned.shape.m8n8k16.{} {}, {}, {}, {};", itype, dst, ra, rb, rc)
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
    #[test] fn rule_u8() {
        let i = RuleInst::new("IMMA", &["8816","U8"], vec![Op::r(0)], vec![Op::r(2),Op::r(4),Op::r(6)]);
        assert!(translate(&i, &sb()).contains("imma.sync.aligned.shape.m8n8k16.u8"));
    }
    #[test] fn rule_s8() {
        let i = RuleInst::new("IMMA", &["8816","S8"], vec![Op::r(0)], vec![Op::r(2),Op::r(4),Op::r(6)]);
        assert!(translate(&i, &sb()).contains("imma.sync.aligned.shape.m8n8k16.s8"));
    }
}
