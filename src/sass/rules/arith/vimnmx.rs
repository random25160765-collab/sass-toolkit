// VIMNMX — variable integer min/max
//   PT  → min.u32   |   !PT → max.u32
//   neg_src0/1 → negate corresponding data operand (Zero → 0, not %r0)
use super::super::helpers;
use super::super::types::{Op, RuleInst, Scratch};

fn fmt_data(op: Option<&Op>) -> String {
    match op { Some(Op::Zero) => "0".into(), other => helpers::opt_int(other) }
}

pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    let dst = helpers::opt_int(inst.dst.first());
    let op  = if matches!(inst.src.get(2), Some(Op::NegPred(_)))
        || inst.modifiers.iter().any(|m| m == "neg_src2")
        { "max" } else { "min" };
    let has_neg = |n: usize| inst.modifiers.iter().any(|m| m == &format!("neg_src{}", n));

    let mut lines = Vec::new();
    let ra = if has_neg(0) {
        let t = sb.gpr(0);
        lines.push(format!("    sub.u32 {}, 0, {};", t, fmt_data(inst.src.first())));
        t
    } else {
        fmt_data(inst.src.first())
    };
    let rb = if has_neg(1) {
        let t = sb.gpr(1);
        lines.push(format!("    sub.u32 {}, 0, {};", t, fmt_data(inst.src.get(1))));
        t
    } else {
        fmt_data(inst.src.get(1))
    };
    lines.push(format!("    {}.u32 {}, {}, {};", op, dst, ra, rb));
    if lines.len() == 1 { lines.into_iter().next().unwrap() }
    else {
        lines.join("\n").replace("    ", "    ")
    }
}
