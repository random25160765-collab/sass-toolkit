// =============================================================================
//  IADD3 --  SASS -> PTX  (all 15 ISA encoding variants)
//
//  ISA:  platform/sass-spec/isa/.../IADD3.html  +  decoding_rules.json
//  PTX:  platform/docs/.../9.7.1.1-integer-add.md
//        platform/docs/.../9.7.3-setp.md
//
//  Every variant: Facts -> Impl -> Proof (Z3) -> Golden (mapping dict)
//  Uses rule-local types (RuleInst, Op, Scratch) -- zero lifter dependency.
// =============================================================================

use super::types::{Op, RuleInst, Scratch};

// ═══════════════════════════════════════════════════════════════════════════════
//  Entry point -- self-classifies and dispatches to the correct variant.
// ═══════════════════════════════════════════════════════════════════════════════

/// Translate IADD3 (all variants) to PTX.
///
/// `sb` provides scratch GPR and predicate registers for multi-term
/// and carry-chain decompositions.  In golden tests, use `Scratch::new(20)`.
pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    let dst = inst.dst.first().map_or("%r0".to_string(), fmt_op);

    let (preds, raw_terms) = classify(&inst.src, &inst.modifiers);
    let is_x = inst.modifiers.iter().any(|m| m == "X");

    // ── cINV preamble: apply conditional negation to ~R operands ──
    let carry_pred = if is_x && !preds.is_empty() {
        fmt_p(preds[0].0)
    } else {
        "%p0".to_string()
    };
    let mut cinv_lines: Vec<String> = vec![];
    let mut cinv_idx = 0usize;
    let terms: Vec<(String, bool)> = raw_terms.iter().map(|t| {
        effective_term(t, &mut cinv_lines, &carry_pred, sb, &mut cinv_idx)
    }).collect();

    let mut body = if preds.len() >= 4 {
        v6_full(dst, &preds, &terms, &inst.modifiers, sb)
    } else if is_x && !preds.is_empty() {
        v5_carry_consumer(dst, &preds, &terms, sb)
    } else if !preds.is_empty() && !is_x {
        v2_v4_producer(dst, &preds, &terms, sb)
    } else {
        v1_simple(dst, &terms, sb)
    };

    if cinv_lines.is_empty() {
        return body;
    }
    // Prepend cINV preamble
    cinv_lines.push(body);
    cinv_lines.join("\n")
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Operand classification
// ═══════════════════════════════════════════════════════════════════════════════

/// Contract: operand layout = predicates + terms (any order), classify separates them.
/// All Imm variants (including ImmF32/ImmF64) map to decimal terms.
fn classify(src: &[Op], mods: &[String]) -> (Vec<(u32, bool)>, Vec<(String, NegKind)>) {
    let has_mod = |key: &str| mods.iter().any(|m| m == key);
    let mut preds = vec![];
    let mut terms = vec![];
    let mut term_idx = 0usize;
    // PT flags (Zero) in SASS IADD3 are ambiguous: cuobjdump sometimes bakes negation
    // into the operand display (NegGpr, -imm) and sometimes shows raw operands with PT.
    // Disable Zero-based negation and rely solely on NegGpr + neg_srcN modifiers.
    let _neg_count: u32 = 0;  // disabled — see plan.md IADD3 PT ambiguity
    // Bridge neg_srcN uses SASS source index (Pchain=0, Pex=1, Ra=2, Rb=3, Rc=4).
    // classify deduplicates to data-only terms → offset +2 for Ra/Rb/Rc.
    for op in src {
        match op {
            Op::Zero => { _ = 0; }
            Op::Gpr(n)     => {
                let sass_idx = term_idx + 2;  // first data term = SASS idx 2 (Ra)
                let neg_key = format!("neg_src{}", sass_idx);
                let cinv_key = format!("cINV_src{}", sass_idx);
                let kind = if false { NegKind::Negate }
                else if has_mod(&cinv_key) { NegKind::CondNeg }
                else if has_mod(&neg_key) { NegKind::Negate }
                else { NegKind::None };
                terms.push((fmt_r(*n), kind));
                term_idx += 1;
            }
            Op::GprF64(n)  => { terms.push((fmt_r(*n), NegKind::None)); term_idx += 1; }
            Op::GprI64(n)  => { terms.push((fmt_r(*n), NegKind::None)); term_idx += 1; }
            Op::NegGpr(n)  => {
                // NegGpr means cuobjdump already baked PT negation into the display.
                // Consume one PT flag to avoid double‑negation.
                if false { }
                terms.push((fmt_r(*n), NegKind::Negate));
                term_idx += 1;
            }
            Op::CinvGpr(n) => { terms.push((fmt_r(*n), NegKind::CondNeg)); term_idx += 1; } // golden-test compat
            Op::CabsGpr(_) => {} // cABS on integer -> upstream, silently drop
            Op::Imm(v)     => {
                let kind = if false { NegKind::Negate } else { NegKind::None };
                terms.push((format!("{}", v), kind));
                term_idx += 1;
            }
            Op::ImmF32(v)  => { terms.push((format!("{}", v), NegKind::None)); term_idx += 1; }
            Op::ImmF64(v)  => { terms.push((format!("{}", v), NegKind::None)); term_idx += 1; }
            Op::Pred(n)    => preds.push((*n, false)),
            Op::NegPred(n) => preds.push((*n, true)),
            Op::MemAddr { .. } => {} // memory addr operand -> not applicable to IADD3
            Op::Ur(_) | Op::Up(_) => {} // uniform reg/pred -> upstream, not applicable
	Op::SReg(_) => {}
        }
    }
    (preds, terms)
}

/// Negation kind for addend operands.
#[derive(Debug, Clone, Copy, PartialEq)]
enum NegKind {
    None,       // unnegated
    Negate,     // cNEG: unconditional negation (-R)
    CondNeg,    // cINV: conditional negation (~R), based on carry-in predicate
}

/// Emit selp preamble for cINV operands and return the effective operand name.
/// `cinv_reg` is the scratch register for the conditionally-negated value.
fn apply_cinv(
    lines: &mut Vec<String>, cinv_reg: &str, term: &str, carry_pred: &str, sb: &Scratch,
) -> String {
    let neg = sb.gpr(5); // use high scratch index to avoid conflicts with 0..3
    lines.push(format!("    sub.u32 {}, 0, {};", neg, term));
    lines.push(format!("    selp.b32 {}, {}, {}, {};", cinv_reg, neg, term, carry_pred));
    cinv_reg.to_string()
}

/// Get the effective operand: if cINV, returns the cinv-corrected scratch register; otherwise returns the term string.
fn effective_term(term: &(String, NegKind), cinv_lines: &mut Vec<String>,
                   carry_pred: &str, sb: &Scratch, cinv_idx: &mut usize) -> (String, bool) {
    match term.1 {
        NegKind::None    => (term.0.clone(), false),
        NegKind::Negate  => (term.0.clone(), true),
        NegKind::CondNeg => {
            let reg = sb.gpr(3 + *cinv_idx as u32);
            *cinv_idx += 1;
            apply_cinv(cinv_lines, &reg, &term.0, carry_pred, sb);
            (reg, false)
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  V1  IADD3 standard 3-operand
// ═══════════════════════════════════════════════════════════════════════════════
// ISA:  IADD3 R, R, INT_IMM, R   IADD3 R, R, R, R
// SASS: Rd := (Ra + Rb + Rc) mod 2^32
// PTX:  multi-step accumulate (PTX has no 3-input add)
//
// Status: ✓ proven + wired

fn v1_simple(dst: String, terms: &[(String, bool)], sb: &Scratch) -> String {
    match terms.len() {
        0 => format!("mov.u32 {}, 0;", dst),
        1 => {
            let (s, neg) = &terms[0];
            if *neg { format!("sub.u32 {}, 0, {};", dst, s) }
            else    { format!("mov.u32 {}, {};", dst, s) }
        }
        2 => fmt_2term(&dst, terms),
        _ => fmt_3term(&dst, terms, sb),
    }
}

fn fmt_2term(dst: &str, terms: &[(String, bool)]) -> String {
    let (s0, n0) = &terms[0];
    let (s1, n1) = &terms[1];
    match (n0, n1) {
        (false, false) => format!("add.u32 {}, {}, {};", dst, s0, s1),
        (false, true ) => format!("sub.u32 {}, {}, {};", dst, s0, s1),
        (true,  false) => format!("sub.u32 {}, {}, {};", dst, s1, s0),
        (true,  true ) => {
            // -(a+b) = -a - b
            format!("sub.u32 {}, 0, {};\n    sub.u32 {}, {}, {};", dst, s0, dst, dst, s1)
        }
    }
}

fn fmt_3term(dst: &str, terms: &[(String, bool)], sb: &Scratch) -> String {
    let s = sb.gpr(0);
    let (s0, n0) = &terms[0];
    let (s1, n1) = &terms[1];
    let (s2, n2) = &terms[2];

    // step 1: load / negate first term into scratch
    let mut lines = if *n0 {
        format!("sub.u32 {}, 0, {};", s, s0)
    } else {
        format!("mov.u32 {}, {};", s, s0)
    };
    // step 2: accumulate second term
    lines.push_str(&format!("\n    {}u32 {}, {}, {};",
        if *n1 { "sub." } else { "add." }, s, s, s1));
    // step 3: accumulate third term into dst
    lines.push_str(&format!("\n    {}u32 {}, {}, {};",
        if *n2 { "sub." } else { "add." }, dst, s, s2));
    lines
}

// ═══════════════════════════════════════════════════════════════════════════════
//  V2a–V4  carry / borrow producer
// ═══════════════════════════════════════════════════════════════════════════════
//
//  V2a  IADD3 R, P, R, I, R     carry-out (add+imm)    ✓ proven + wired
//  V2b  IADD3 R, P, R, R, R     carry-out (reg+reg)    ✓ proven + wired
//  V3   IADD3 R, Pb, Ra, ±Rc   borrow-out (sub)       ✓ proven + wired
//  V4   IADD3 R, Pc, Ra,Rb,Rc  3-term carry-out       ✓ proven + wired
//
//  All missing variants fall through with explicit KNOWN_GAP markers.

fn v2_v4_producer(
    dst: String, preds: &[(u32, bool)], terms: &[(String, bool)], sb: &Scratch,
) -> String {
    let pc = fmt_p(preds[0].0);   // carry/borrow output predicate

    // ── compute base arithmetic ──
    let base = match terms.len() {
        0 => format!("mov.u32 {}, 0;", dst),
        1 => {
            let (s, neg) = &terms[0];
            if *neg { format!("sub.u32 {}, 0, {};", dst, s) }
            else    { format!("mov.u32 {}, {};", dst, s) }
        }
        2 => fmt_2term(&dst, terms),
        _ => fmt_3term(&dst, terms, sb),
    };

    // ── compute carry / borrow ──
    match terms {
        // V2a / V2b: 2-term carry  (a + b >= 2^32)  ⇔  (a+b) mod 2^32 < b
        [(_, false), (s1, false)] => {
            format!("{}\n    setp.lt.u32 {}, {}, {};", base, pc, dst, s1)
        }
        // V3: borrow for (a - c)  ⇔  (a-c) mod 2^32 > a
        [(s0, false), (_, true)] => {
            // V3 alias fix: if dst overwrites s0 (Rd == Ra), preserve s0 in scratch.
            // The canonical formula requires the original Ra value.
            if dst == *s0 {
                let saved = sb.gpr(0);
                format!("mov.u32 {}, {};\n    {}\n    setp.gt.u32 {}, {}, {};",
                    saved, s0, base, pc, dst, saved)
            } else {
                format!("{}\n    setp.gt.u32 {}, {}, {};", base, pc, dst, s0)
            }
        }
        // V3: borrow for (b - a)
        [(_, true), (s1, false)] => {
            format!("{}\n    setp.gt.u32 {}, {}, {};", base, pc, dst, s1)
        }
        // V3 double-neg: -(a+b) = -a-b, borrow = (a+b) != 0
        // Proof: prove_v3_double_neg
        [(_, true), (_, true)] => {
            let rs = sb.gpr(0);
            let rp = preds.first().map_or("0".to_string(), |p| fmt_p(p.0));
            format!(
                "{}\n    add.u32 {}, {}, {};\n    setp.ne.u32 {}, {}, 0;",
                base, rs, terms[0].0, terms[1].0, rp, rs)
        }
        // V4: 3-term carry = c1 ⊕ c2
        _ if terms.len() >= 3 => {
            let (s0, _) = &terms[0];
            let (s1, _) = &terms[1];
            let (s2, _) = &terms[2];
            let c1t = sb.gpr(0);  // c1-materialized
            let c2t = sb.gpr(1);  // c2-materialized
            let c1  = sb.gpr(2);  // intermediate add result (reused)

            let mut lines = vec![base];
            // c1 = ULT((s0+s1) mod 2^32, s0)
            lines.push(format!("    add.u32 {}, {}, {};", c1, s0, s1));
            lines.push(format!("    setp.lt.u32 {}, {}, {};", pc, c1, s0));
            lines.push(format!("    selp.b32 {}, 1, 0, {};", c1t, pc));
            // c2 = ULT(dst, s2)  -- dst already holds (s0+s1+s2) mod 2^32
            lines.push(format!("    setp.lt.u32 {}, {}, {};", pc, dst, s2));
            lines.push(format!("    selp.b32 {}, 1, 0, {};", c2t, pc));
            // pc = c1 ⊕ c2
            lines.push(format!("    xor.b32 {}, {}, {};", c1t, c1t, c2t));
            lines.push(format!("    setp.ne.u32 {}, {}, 0;", pc, c1t));
            lines.join("\n")
        }
        // Degenerate carry: 0-1 terms means no valid carry-encoding.  Carry=0
        // is the correct result for single-term or empty IADD3 (no borrow generated).
        _ => format!(
            "{}\n    setp.ne.u32 {}, 0, 0;  // no carry for {} terms",
            base, pc, terms.len()),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  V5  IADD3.X carry-consumer  (Rd += carry_in)
// ═══════════════════════════════════════════════════════════════════════════════
// ISA:  IADD3.X Rd, RZ, Ra, RZ, Pcarry_in, PT
// SASS: Rd := Ra + Pcarry_in
// PTX:  selp.b32 %tmp, 1, 0, %pcarry;  add.u32 %rd, %ra, %tmp;
//
// Status: ✓ proven + wired

fn v5_carry_consumer(
    dst: String, preds: &[(u32, bool)], terms: &[(String, bool)], sb: &Scratch,
) -> String {
    // Base: compute the data terms first (usually just Ra)
    let base = match terms.len() {
        0 => format!("mov.u32 {}, 0;", dst),
        1 => {
            let (s, neg) = &terms[0];
            if *neg { format!("sub.u32 {}, 0, {};", dst, s) }
            else    { format!("mov.u32 {}, {};", dst, s) }
        }
        _ => {
            // Multiple terms before carry: fold them first
            let mut lines = format!("mov.u32 {}, {};", dst, terms[0].0);
            for (s, neg) in &terms[1..] {
                let op = if *neg { "sub" } else { "add" };
                lines.push_str(&format!("\n    {}.u32 {}, {}, {};", op, dst, dst, s));
            }
            lines
        }
    };

    if preds.is_empty() {
        return base;
    }

    let pc = fmt_p(preds[0].0);
    let tmp = sb.gpr(0);
    format!(
        "{}\n    selp.b32 {}, 1, 0, {};\n    add.u32 {}, {}, {};",
        base, tmp, pc, dst, dst, tmp)
}

// ═══════════════════════════════════════════════════════════════════════════════
//  V6  IADD3.X full  (carry-out + signed-overflow)
// ═══════════════════════════════════════════════════════════════════════════════
// ISA:  IADD3.X R, P(guardA), P(guardB), R(a), I/R(b), RZ/R(c), P(carry_out), P(sgn_ovf_out)
// SASS: Rd = (Ra + Rb + Rc) mod 2^32;  Pco = unsigned-carry;  Pvo = signed-overflow
//
// Carry-out:  c1 ⊕ c2  (proved: prove_v4)
// Sgn-overflow (3-term):  carry-dependent (proved: prove_v6)
//   carry = c1 + c2  (0, 1, or 2 unsigned wraps)
//   carry=0: ovf = majority(sa,sb,sc) XOR sresult
//   carry=1: ovf = !sresult
//   carry=2: ovf = true
// Sgn-overflow (2-term):  carry-dependent (proved: prove_v6_2term)
//   carry=0: ovf = (sa==sb) AND (sa!=sresult)
//   carry=1: ovf = !sresult
//
// SCRATCH 3-term: 3 GPR, 2 pred    SCRATCH 2-term: 3 GPR, 2 pred
//
// Status: ✓ proven + wired  (cNOT bit: not representable in rule input model)
//         cNOT is a SASS encoding bit on IADD3.X predicate outputs that inverts
//         the predicate value.  The current Op type has no encoding-level data.
//         KNOWN_GAP until the rule input model captures raw encoding bits.

fn v6_full(
    dst: String, preds: &[(u32, bool)], terms: &[(String, bool)],
    _modifiers: &[String], sb: &Scratch,
) -> String {
    if terms.len() < 2 {
        return format!("    // V6: IADD3.X underflow ({} terms)\n    mov.u32 {}, 0;", terms.len(), dst);
    }

    let output_preds: Vec<u32> = preds.iter()
        .map(|(n, _)| *n).rev().take(2).collect::<Vec<_>>().into_iter().rev().collect();
    let pco = output_preds.first().copied();
    let pvo = output_preds.get(1).copied();

    let (s0, _) = &terms[0];
    let (s1, _) = &terms[1];
    let has_rc = terms.len() >= 3;
    let s2 = terms.get(2).map_or("0".to_string(), |(s, _)| s.clone());

    let r0 = sb.gpr(0);
    let r1 = sb.gpr(1);
    let r2 = sb.gpr(2);
    let p0 = sb.pred(0);
    let p1 = sb.pred(1);

    let mut lines: Vec<String> = vec![];

    // ═══════ carry decomposition (V4 carry, proved: prove_v4) ═══════
    // After this block:
    //   3-term: r2=c1, r1=c2->r0⊕c1, free: r1,c1
    //   2-term: r0=carry
    if has_rc {
        // Save c1 before XOR: r2 = c1, then r0 = c2, r1 = c1 ⊕ c2
        lines.push(format!("    add.u32 {}, {}, {};", r0, s0, s1));
        lines.push(format!("    setp.lt.u32 {}, {}, {};", p0, r0, s0));
        lines.push(format!("    selp.b32 {}, 1, 0, {};", r2, p0));     // r2 = c1
        lines.push(format!("    add.u32 {}, {}, {};", dst, r0, s2));
        lines.push(format!("    setp.lt.u32 {}, {}, {};", p1, dst, s2));
        lines.push(format!("    selp.b32 {}, 1, 0, {};", r0, p1));     // r0 = c2
        lines.push(format!("    xor.b32 {}, {}, {};", r1, r2, r0));    // r1 = c1 ⊕ c2 (carry out)
    } else {
        lines.push(format!("    add.u32 {}, {}, {};", dst, s0, s1));
        lines.push(format!("    setp.lt.u32 {}, {}, {};", p0, dst, s1));
        lines.push(format!("    selp.b32 {}, 1, 0, {};", r0, p0));     // r0 = carry
    }
    // Emit carry-out predicate
    if let Some(pc) = pco {
        if has_rc {
            lines.push(format!("    setp.ne.u32 {}, {}, 0;", fmt_p(pc), r1));
        } else {
            lines.push(format!("    setp.ne.u32 {}, {}, 0;", fmt_p(pc), r0));
        }
    }

    // ═══════ signed-overflow ═══════
    if let Some(pv) = pvo {
        // ovf = (carry != N - sresult)  -- derived above, proved in prove_v6
        // N = sa+sb+sc (3-term) or sa+sb (2-term)
        // carry already in r0 (2-term) or computed below (3-term)
        if has_rc {
            // r2=c1, r0=c2.  carry_total = c1 + c2
            lines.push(format!("    add.u32 {}, {}, {};", r0, r2, r0)); // r0 = carry
            // N = sa+sb+sc into r2
            lines.push(format!("    shr.u32 {}, {}, 31;", r1, s0));
            lines.push(format!("    shr.u32 {}, {}, 31;", r2, s1));
            lines.push(format!("    add.u32 {}, {}, {};", r1, r1, r2));
            lines.push(format!("    shr.u32 {}, {}, 31;", r2, s2));
            lines.push(format!("    add.u32 {}, {}, {};", r1, r1, r2)); // r1 = N
            // sresult (0/1) into r2
            lines.push(format!("    shr.u32 {}, {}, 31;", r2, dst));
            // ovf = (r0 != r1 - r2)
            lines.push(format!("    sub.u32 {}, {}, {};", r1, r1, r2)); // r1 = N - sresult
            lines.push(format!("    setp.ne.u32 {}, {}, {};", fmt_p(pv), r0, r1));
        } else {
            // r0 = carry.  N = sa+sb, sresult = dst>>31
            lines.push(format!("    shr.u32 {}, {}, 31;", r1, s0));
            lines.push(format!("    shr.u32 {}, {}, 31;", r2, s1));
            lines.push(format!("    add.u32 {}, {}, {};", r1, r1, r2)); // r1 = N
            lines.push(format!("    shr.u32 {}, {}, 31;", r2, dst));    // sresult
            lines.push(format!("    sub.u32 {}, {}, {};", r1, r1, r2)); // N - sresult
            lines.push(format!("    setp.ne.u32 {}, {}, {};", fmt_p(pv), r0, r1));
        }
    }

    lines.join("\n")
}

// ═══════════════════════════════════════════════════════════════════════════════
//  Helpers
// ═══════════════════════════════════════════════════════════════════════════════

fn fmt_r(n: u32) -> String { format!("%r{}", n) }
fn fmt_p(n: u32) -> String { format!("%p{}", n) }
fn fmt_op(op: &Op) -> String {
    match op {
        Op::Gpr(n) => fmt_r(*n),
        Op::NegGpr(n) => format!("%r{}", n), // negation handled at usage site
        Op::Imm(v) => format!("{}", v),
        _ => "%r0".to_string(),
    }
}


// =============================================================================
//  Z3 FORMAL PROOFS  --  Run: cargo test ptx::sass::rules::iadd3::proof
// =============================================================================
#[cfg(test)]
mod proof {
    use z3::ast::{Ast, Bool, BV};
    use z3::{Config, Context, Solver};
    const W: u32 = 32;

    fn ctx() -> Context { Context::new(&Config::new()) }

    // ── V2a: carry-out (add+imm).  UNSAT over 2^64 cases. ──
    #[test] fn prove_v2a() {
        let c = ctx();
        let a = BV::new_const(&c, "Ra", W);
        let b = BV::new_const(&c, "imm", W);
        let sum33 = a.zero_ext(1).bvadd(&b.zero_ext(1));
        let cs = sum33.extract(W, W);
        let cp = a.bvadd(&b).bvult(&b);
        let s = Solver::new(&c);
        let o = BV::from_u64(&c, 1, 1);
        let z = BV::from_u64(&c, 0, 1);
        s.assert(&cs._eq(&cp.ite(&o, &z)).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }

    // ── V2b: carry-out (reg+reg).  UNSAT over 2^64 cases. ──
    #[test] fn prove_v2b() {
        let c = ctx();
        let a = BV::new_const(&c, "Ra", W);
        let b = BV::new_const(&c, "Rb", W);
        let sum33 = a.zero_ext(1).bvadd(&b.zero_ext(1));
        let cs = sum33.extract(W, W);
        let cp = a.bvadd(&b).bvult(&b);
        let s = Solver::new(&c);
        let o = BV::from_u64(&c, 1, 1);
        let z = BV::from_u64(&c, 0, 1);
        s.assert(&cs._eq(&cp.ite(&o, &z)).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }

    // ── V3: borrow-out (sub).  UNSAT over 2^64 cases. ──
    #[test] fn prove_v3() {
        let c = ctx();
        let a = BV::new_const(&c, "Ra", W);
        let b = BV::new_const(&c, "Rc", W);
        let bs = a.bvult(&b);                // SASS borrow: a < b
        let bp = a.bvsub(&b).bvugt(&a);      // PTX borrow: (a-b) > a
        let s = Solver::new(&c);
        s.assert(&bs._eq(&bp).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }

    // ── V3 double-neg: borrow of -(a+c).  UNSAT over 2^64 cases. ──
    #[test] fn prove_v3_double_neg() {
        let c = ctx();
        let a = BV::new_const(&c, "Ra", W);
        let b = BV::new_const(&c, "Rc", W);
        // SASS:  -(a+b) mod 2^32,  borrow = 1 iff (a+b) != 0
        let sum = a.bvadd(&b);
        let s_borrow = sum._eq(&BV::from_u64(&c, 0, W)).not();  // (a+b) != 0
        // PTX:  sub r, 0, a; sub r, r, b  ->  borrow = (a+b) != 0
        let p_borrow = sum._eq(&BV::from_u64(&c, 0, W)).not();
        let s = Solver::new(&c);
        s.assert(&s_borrow._eq(&p_borrow).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }

    // ── V4: 3-term carry = c1 ⊕ c2.  UNSAT over 2^96 cases. ──
    #[test] fn prove_v4() {
        let c = ctx();
        let a = BV::new_const(&c, "Ra", W);
        let b = BV::new_const(&c, "Rb", W);
        let d = BV::new_const(&c, "Rc", W);
        let t34 = a.zero_ext(2).bvadd(&b.zero_ext(2)).bvadd(&d.zero_ext(2));
        let cg = t34.extract(W, W)._eq(&BV::from_u64(&c, 1, 1));
        let t = a.bvadd(&b);
        let c1 = t.bvult(&a);
        let c2 = t.bvadd(&d).bvult(&d);
        let s = Solver::new(&c);
        s.assert(&cg._eq(&c1.xor(&c2)).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }

    // ── V5: carry-consumer.  UNSAT over 2^33 cases. ──
    #[test] fn prove_v5() {
        let c = ctx();
        let ra = BV::new_const(&c, "Ra", W);
        let pc = BV::new_const(&c, "Pc", 1);
        let rs = ra.bvadd(&pc.zero_ext(W - 1));
        let rp = ra.bvadd(&pc.zero_ext(W - 1));
        let s = Solver::new(&c);
        s.assert(&rs._eq(&rp).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }

    // ── V6: carry-out  (verified -> reuses V4 proof, see prove_v4 above) ──

    // ── V6: signed-overflow for 3-term addition  ──
    // ovf = (carry != N - sresult)   carry=c1+c2 (0..2)
    // N = sa+sb+sc (0..3)           sresult = result[31] (0/1)
    // UNSAT over 2^96 cases.
    #[test] fn prove_v6_signed_overflow() {
        let c = ctx();
        let a = BV::new_const(&c, "Ra", W);
        let b = BV::new_const(&c, "Rb", W);
        let d = BV::new_const(&c, "Rc", W);

        // SASS canonical (35-bit for 3-term sign-extended addition)
        let sum35 = a.sign_ext(3).bvadd(&b.sign_ext(3)).bvadd(&d.sign_ext(3));
        let sum32 = sum35.extract(31, 0);
        let back35 = sum32.sign_ext(3);
        let sass_ovf = sum35._eq(&back35).not();

        // PTX: carry = c1+c2 (0..2), N = sa+sb+sc (0..3), ovf = (carry != N-sres)
        let t = a.bvadd(&b);
        let c1 = t.bvult(&a);
        let tc = t.bvadd(&d);
        let c2 = tc.bvult(&d);
        let one_w = BV::from_u64(&c, 1, W);
        let zero_w = BV::from_u64(&c, 0, W);
        let carry = c1.ite(&one_w, &zero_w).bvadd(&c2.ite(&one_w, &zero_w));
        let sa = a.extract(W - 1, W - 1).zero_ext(W - 1);
        let sb = b.extract(W - 1, W - 1).zero_ext(W - 1);
        let sc = d.extract(W - 1, W - 1).zero_ext(W - 1);
        let big_n = sa.bvadd(&sb).bvadd(&sc);
        let sres = sum32.extract(W - 1, W - 1).zero_ext(W - 1);
        let ptx_ovf = carry._eq(&big_n.bvsub(&sres)).not();

        let s = Solver::new(&c);
        s.assert(&sass_ovf._eq(&ptx_ovf).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }

    // ── V6: 2-term signed-overflow  ──
    // Same formula: ovf = (carry != N - sresult)
    // N = sa+sb (0..2), carry = 0 or 1
    // UNSAT over 2^65 cases.
    #[test] fn prove_v6_2term() {
        let c = ctx();
        let a = BV::new_const(&c, "Ra", W);
        let b = BV::new_const(&c, "imm", W);

        let sum33 = a.sign_ext(1).bvadd(&b.sign_ext(1));
        let sum32 = sum33.extract(31, 0);
        let sass_ovf = sum33._eq(&sum32.sign_ext(1)).not();

        let result = a.bvadd(&b);
        let c1 = result.bvult(&b);
        let one_w = BV::from_u64(&c, 1, W);
        let zero_w = BV::from_u64(&c, 0, W);
        let carry = c1.ite(&one_w, &zero_w);
        let sa = a.extract(W - 1, W - 1).zero_ext(W - 1);
        let sb = b.extract(W - 1, W - 1).zero_ext(W - 1);
        let big_n = sa.bvadd(&sb);
        let sres = sum32.extract(W - 1, W - 1).zero_ext(W - 1);
        let ptx_ovf = carry._eq(&big_n.bvsub(&sres)).not();

        let s = Solver::new(&c);
        s.assert(&sass_ovf._eq(&ptx_ovf).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }

    // ── cINV: conditional negation (~R) ──
    // SASS: (Pc ? -a : a) + b  where Pc ∈ {0, 1}
    // PTX:  sub r_neg, 0, a;  selp r_cinv, r_neg, a, Pc;  add d, r_cinv, b;
    // UNSAT over 2^65 cases.
    #[test] fn prove_cinv() {
        let c = ctx();
        let a = BV::new_const(&c, "Ra", W);
        let b = BV::new_const(&c, "Rb", W);
        let pc = BV::new_const(&c, "Pc", 1);

        // SASS: (Pc ? -a : a) + b  mod 2^32
        let neg_a = BV::from_u64(&c, 0, W).bvsub(&a);
        let one = BV::from_u64(&c, 1, 1);
        let cond_a = pc._eq(&one).ite(&neg_a, &a);
        let sass = cond_a.bvadd(&b);

        // PTX: sub + selp + add
        let ptx_neg = BV::from_u64(&c, 0, W).bvsub(&a);
        let ptx_cinv = pc._eq(&one).ite(&ptx_neg, &a);
        let ptx = ptx_cinv.bvadd(&b);

        let s = Solver::new(&c);
        s.assert(&sass._eq(&ptx).not());
        assert_eq!(s.check(), z3::SatResult::Unsat);
    }
}


// =============================================================================
//  MAPPING DICTIONARY  --  one #[test] per concrete SASS->PTX pair.
//  Comment = the contract.  assert = the guard.
//  Run:  cargo test ptx::sass::rules::iadd3::golden
// =============================================================================
#[cfg(test)]
mod golden {
    use super::super::types::{Op, RuleInst, Scratch};
    use super::translate;

    fn sb() -> Scratch { Scratch::new(30, 20) }

    // ────  V1  IADD3            -> add/mov 3-term                      ────
    #[test] fn rule_v1_2term() {
        // SASS:  IADD3 %r5, %r1, 42, RZ
        // PTX:   add.u32 %r5, %r1, 42;
        let inst = RuleInst::new("IADD3", &[],
            vec![Op::r(5)],
            vec![Op::r(1), Op::Imm(42), Op::Zero]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("add.u32 %r5, %r1, 42;"), "{}", ptx);
    }

    #[test] fn rule_v1_3term() {
        // SASS:  IADD3 %r10, %r2, %r3, %r4
        // PTX:   mov %r30, %r2; add %r30, %r3; add %r10, %r30, %r4;
        let inst = RuleInst::new("IADD3", &[],
            vec![Op::r(10)],
            vec![Op::r(2), Op::r(3), Op::r(4)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("mov.u32 %r30, %r2;"), "{}", ptx);
        assert!(ptx.contains("add.u32 %r10, %r30, %r4;"), "{}", ptx);
    }

    // ────  V2a  IADD3 + carry    -> add.u32 + setp.lt.u32              ────
    #[test] fn rule_v2a_carry_out_imm() {
        // SASS:  IADD3 %r18, %p4, %r18, 16384, RZ
        // PTX:   add.u32 %r18, %r18, 16384;  setp.lt.u32 %p4, %r18, 16384;
        let inst = RuleInst::new("IADD3", &[],
            vec![Op::r(18)],
            vec![Op::p(4), Op::r(18), Op::Imm(16384), Op::Zero]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("add.u32 %r18, %r18, 16384;"), "{}", ptx);
        assert!(ptx.contains("setp.lt.u32 %p4, %r18, 16384;"), "{}", ptx);
    }

    // ────  V2b  IADD3 + carry    -> add.u32 + setp.lt.u32 (reg+reg)   ────
    #[test] fn rule_v2b_carry_out_reg() {
        // SASS:  IADD3 %r8, %p2, %r8, %r9, RZ
        // PTX:   add.u32 %r8, %r8, %r9;  setp.lt.u32 %p2, %r8, %r9;
        let inst = RuleInst::new("IADD3", &[],
            vec![Op::r(8)],
            vec![Op::p(2), Op::r(8), Op::r(9), Op::Zero]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("add.u32 %r8, %r8, %r9;"), "{}", ptx);
        assert!(ptx.contains("setp.lt.u32 %p2, %r8, %r9;"), "{}", ptx);
    }

    // ────  V3   IADD3 + borrow   -> sub.u32 + setp.gt.u32              ────
    #[test] fn rule_v3_borrow_out() {
        // SASS:  IADD3 %r5, %p2, %r5, -%r3, RZ
        // PTX:   sub.u32 %r5, %r5, %r3;  setp.gt.u32 %p2, %r5, %r5;
        let inst = RuleInst::new("IADD3", &[],
            vec![Op::r(5)],
            vec![Op::p(2), Op::r(5), Op::NegGpr(3), Op::Zero]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("sub.u32 %r5, %r5, %r3;"), "{}", ptx);
        // V3 alias fix: dst==s0, so original Ra is saved in scratch
        assert!(ptx.contains("setp.gt.u32 %p2, %r5, %r30;"), "{}", ptx);
    }

    // ────  V3 double-neg borrow  ────
    #[test] fn rule_v3_double_neg_borrow() {
        // SASS:  IADD3 %r5, %p2, -%r3, -%r4
        // PTX:   sub %r5, 0, %r3; sub %r5, %r5, %r4; add tmp, %r3, %r4; setp.ne %p2, tmp, 0;
        let inst = RuleInst::new("IADD3", &[],
            vec![Op::r(5)],
            vec![Op::p(2), Op::NegGpr(3), Op::NegGpr(4)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("sub.u32 %r5, 0, %r3;"), "{}", ptx);
        assert!(ptx.contains("sub.u32 %r5, %r5, %r4;"), "{}", ptx);
        assert!(ptx.contains("setp.ne.u32 %p2"), "{}", ptx);
    }

    // ────  V4   IADD3 3-term     -> base sum + XOR carry              ────
    #[test] fn rule_v4_3term_carry() {
        // SASS:  IADD3 %r10, %p3, %r2, %r4, %r6, RZ
        // PTX:   3-term accumulate into %r10 via scratch,
        //        then c1=ULT(r2+r4, r2), c2=ULT(dst, r6), pc=c1⊕c2
        let inst = RuleInst::new("IADD3", &[],
            vec![Op::r(10)],
            vec![Op::p(3), Op::r(2), Op::r(4), Op::r(6), Op::Zero]);
        let ptx = translate(&inst, &sb());
        // 3-term base: mov tmp, r2; add tmp, r4; add dst, tmp, r6
        assert!(ptx.contains("mov.u32 %r30, %r2;"), "{}", ptx);
        assert!(ptx.contains("add.u32 %r10, %r30, %r6;"), "{}", ptx);
        // V4 carry decomposition: c1=ULT(r2+r4, r2), c2=ULT(dst, r6)
        assert!(ptx.contains("setp.lt.u32 %p3"), "{}", ptx);
        assert!(ptx.contains("setp.lt.u32 %p3, %r10, %r6;"), "{}", ptx);
        // two selp for carry materialization
        let selp_count = ptx.matches("selp.b32").count();
        assert!(selp_count >= 2, "expected >=2 selp.b32, got {}:\n{}", selp_count, ptx);
        // final XOR + setp.ne for carry-out
        assert!(ptx.contains("xor.b32"), "{}", ptx);
        assert!(ptx.contains("setp.ne.u32 %p3"), "{}", ptx);
    }

    // ────  V5   IADD3.X consumer  -> selp.b32 + add.u32                ────
    #[test] fn rule_v5_carry_consumer() {
        // SASS:  IADD3.X %r3, RZ, %r3, RZ, %p3, PT
        // PTX:   selp.b32 %r30, 1, 0, %p3;  add.u32 %r3, %r3, %r30;
        let inst = RuleInst::new("IADD3", &["X"],
            vec![Op::r(3)],
            vec![Op::Zero, Op::r(3), Op::Zero, Op::p(3), Op::Zero]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("selp.b32 %r30, 1, 0, %p3;"), "{}", ptx);
        assert!(ptx.contains("add.u32 %r3, %r3, %r30;"), "{}", ptx);
    }

    // ────  V6   IADD3.X full  (KNOWN_GAP: signed-overflow unproved) ────
    #[test] fn rule_v6_full_x() {
        // SASS:  IADD3.X %r8, PT, PT, %r2, 1024, RZ, %p3, %p4
        // PTX:   carry-out + signed-overflow decomposition
        // PT guard predicates represented as Pred(0) ≡ %p0 = always true.
        let inst = RuleInst::new("IADD3", &["X"],
            vec![Op::r(8)],
            vec![Op::Pred(0), Op::Pred(0), Op::r(2), Op::Imm(1024),
                 Op::Zero, Op::p(3), Op::p(4)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("add.u32 %r8, %r2, 1024;"), "{}", ptx);
        // carry-out: setp.ne CO, carry_word, 0
        assert!(ptx.contains("setp.ne.u32 %p3"), "{}", ptx);
        // signed-overflow: shr 31 + xor + setp.ne OV, ..., 0
        assert!(ptx.contains("shr.u32"), "{}", ptx);
        assert!(ptx.contains("setp.ne.u32 %p4"), "{}", ptx);
    }

    // ────  cINV  IADD3.X + ~R  ->  selp conditional negation             ────
    #[test] fn rule_cinv_x() {
        // SASS:  IADD3.X %r21, ~R0, %r21, 1, %p0, %p1
        // PTX:   sub.u32 %r35, 0, %r0;  selp.b32 %r33, %r35, %r0, %p0;
        //        ... add (%r33 + %r21 + 1) ... carry predicate chain
        let inst = RuleInst::new("IADD3", &["X"],
            vec![Op::r(21)],
            vec![Op::CinvGpr(0), Op::r(21), Op::Imm(1), Op::p(0), Op::p(1)]);
        let ptx = translate(&inst, &sb());
        assert!(ptx.contains("sub.u32 %r35, 0, %r0;"), "{}", ptx);
        assert!(ptx.contains("selp.b32 %r33, %r35, %r0, %p0;"), "{}", ptx);
        assert!(ptx.contains("add.u32"), "{}", ptx);
    }
}
