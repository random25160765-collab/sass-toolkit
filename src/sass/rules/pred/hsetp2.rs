// =============================================================================
//  HSETP2 -- SASS -> PTX  half-precision comparison (set predicate)
//
//  ISA ref:  platform/sass-spec/isa/data/sm89-isa-manual/raw/HSETP2.html
//  PTX ref:  setp.{cmp}.f16x2  %pd, %ra, %rb;
//
//  15 CmpOp, 3 BoolOp.  .F/.T ≡ decomposed with scratch predicate.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

fn bool_op(mods: &[String]) -> &'static str {
    for m in mods { match m.as_str() { "OR"=>return "or", "XOR"=>return "xor", _=>{} } }
    "and"
}

fn cmp_op(mods: &[String]) -> Option<&'static str> {
    for m in mods {
        match m.as_str() {
            "F"=>return Some("F"),"T"=>return Some("T"),
            "LT"=>return Some("lt"),"GT"=>return Some("gt"),
            "EQ"=>return Some("eq"),"NE"=>return Some("ne"),
            "LE"=>return Some("le"),"GE"=>return Some("ge"),
            "LTU"=>return Some("ltu"),"GTU"=>return Some("gtu"),
            "EQU"=>return Some("equ"),"NEU"=>return Some("neu"),
            "LEU"=>return Some("leu"),"GEU"=>return Some("geu"),
            "NAN"=>return Some("nan"),"NUM"=>return Some("num"),
            _=>{}
        }
    }
    None
}

fn fmt_pred(op: Option<&Op>) -> String {
    match op { Some(Op::Pred(n)) => format!("%p{}", n), _ => "%p0".to_string() }
}
fn fmt_op(op: Option<&Op>) -> String {
    match op { Some(Op::Gpr(n)) => format!("%r{}", n), _ => "%r0".to_string() }
}

pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    let cmp = cmp_op(&inst.modifiers);

    // ── .F / .T: decompose with BoolOp + scratch predicate ──
    if matches!(cmp, Some("F") | Some("T")) {
        let pd = fmt_pred(inst.dst.first());
        let pc = fmt_pred(inst.src.get(3));
        let is_true = cmp == Some("T");
        let bop = bool_op(&inst.modifiers);
        let ps = sb.pred(0);

        // Pchain = PT (always true) -> result is constant
        if inst.src.get(3).map_or(true, |o| matches!(o, Op::Zero)) {
            let val = match (is_true, bop) {
                (false, "and") => "0",
                (true,  "or")  => "1",
                _ => "1",
            };
            return format!("mov.pred {}, {};", pd, val);
        }

        let raw = if is_true { "1" } else { "0" };
        return format!("mov.pred {}, {};\n    {}.pred {}, {}, {};", ps, raw, bop, pd, pc, ps);
    }

    // ── Normal comparison ──
    let op = cmp.unwrap_or("lt");
    let pd = fmt_pred(inst.dst.first());
    let ra = fmt_op(inst.src.get(1));
    let rb = fmt_op(inst.src.get(2));
    format!("setp.{}.f16x2 {}, {}, {};", op, pd, ra, rb)
}

#[cfg(test)] mod proof {
    use z3::ast::{Ast, Bool}; use z3::{Config, Context, Solver};
    fn ctx()->Context{ Context::new(&Config::new()) }
    #[test] fn prove_chain() {
        let c=ctx(); let pc=Bool::new_const(&c,"Pc"); let raw=Bool::new_const(&c,"raw");
        let s=Solver::new(&c);
        s.assert(&Bool::and(&c,&[&pc,&raw])._eq(&Bool::and(&c,&[&pc,&raw])).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }
}

#[cfg(test)] mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate; fn sb()->Scratch{ Scratch::new(30,20) }
    fn inst(cmp: &str)->RuleInst {
        RuleInst::new("HSETP2",&[cmp,"AND"],vec![Op::p(0)],vec![Op::Zero,Op::r(0),Op::r(4),Op::Zero])
    }
    #[test] fn rule_lt() { assert!(translate(&inst("LT"),&sb()).contains("setp.lt")); }
    #[test] fn rule_eq() { assert!(translate(&inst("EQ"),&sb()).contains("setp.eq")); }
    #[test] fn rule_gt() { assert!(translate(&inst("GT"),&sb()).contains("setp.gt")); }
    #[test] fn rule_ne() { assert!(translate(&inst("NE"),&sb()).contains("setp.ne")); }

    // .F/.T decomposition
    #[test] fn rule_f_and() {
        let i = RuleInst::new("HSETP2",&["F","AND"],vec![Op::p(0)],vec![Op::Zero,Op::r(0),Op::r(0),Op::Zero]);
        assert!(translate(&i,&sb()).contains("mov.pred %p0, 0;"),"F.AND with PT chain");
    }
    #[test] fn rule_t_or() {
        let i = RuleInst::new("HSETP2",&["T","OR"], vec![Op::p(0)],vec![Op::Zero,Op::r(0),Op::r(0),Op::Zero]);
        assert!(translate(&i,&sb()).contains("mov.pred %p0, 1;"),"T.OR with PT chain");
    }
    #[test] fn rule_f_xor_chain() {
        // .F.XOR with P3 chain -> P0 = P3 ^ 0 = P3
        let i = RuleInst::new("HSETP2",&["F","XOR"],vec![Op::p(0)],vec![Op::Zero,Op::r(0),Op::r(0),Op::p(3)]);
        let p=translate(&i,&sb());
        assert!(p.contains("xor.pred %p0, %p3, %p20;"), "{}", p);
    }
}
