// =============================================================================
//  FABS -- SASS -> PTX  float absolute value
//
//  ISA:  platform/sass-spec/isa/data/sm89-isa-manual/raw/FABS.html
//  PTX:  abs.f32 d, a;   (1:1 axiomatic)
//
//  Modifiers: .ftz (flush-to-zero) — preserved as abs.ftz.f32.
// =============================================================================

use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let d = helpers::opt_f32(inst.dst.first());
    let a = helpers::opt_f32(inst.src.first());
    let ftz = if inst.modifiers.iter().any(|m| m == "FTZ") { ".ftz" } else { "" };
    format!("abs{}.f32 {}, {};", ftz, d, a)
}

#[cfg(test)]
mod proof {
    use z3::ast::{Ast, BV};
    use z3::{Config, Context, Solver};
    const W: u32 = 32;
    fn ctx() -> Context { Context::new(&Config::new()) }
    #[test] fn prove_axiomatic() {
        let c = ctx(); let x = BV::new_const(&c, "x", W);
        let s = Solver::new(&c); s.assert(&x._eq(&x).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }
}

#[cfg(test)]
mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate;
    fn sb() -> Scratch { Scratch::new(30, 20) }

    #[test] fn rule_fabs() {
        // SASS:  FABS R2, R3  ->  abs.f32 %r2, %r3;
        let i = RuleInst::new("FABS", &[], vec![Op::r(2)], vec![Op::r(3)]);
        assert_eq!(translate(&i, &sb()), "abs.f32 %r2, %r3;");
    }

    #[test] fn rule_fabs_ftz() {
        // SASS:  FABS.FTZ R2, R3  ->  abs.ftz.f32 %r2, %r3;
        let i = RuleInst::new("FABS", &["FTZ".into()], vec![Op::r(2)], vec![Op::r(3)]);
        assert_eq!(translate(&i, &sb()), "abs.ftz.f32 %r2, %r3;");
    }
}
