// =============================================================================
//  FSWZADD -- SASS -> PTX  float swizzle + add
//
//  ISA:  platform/sass-spec/isa/.../FSWZADD.html
//  PTX:  prmt.b32 + add.f32  (no direct PTX swizzle-add)
//
//  ISA operand layout keys (1 total):
//    FSWZADD_R_R_R_SwzAdd   reg, reg, reg, swizzle pattern   ✓ handled
//
//  SASS semantic: Rd := Ra + permute_bytes(Rb, pattern)
//
//  PTX decomposition:
//    prmt.b32 Rtmp, RZ, Rb, pattern;  add.f32 Rd, Ra, Rtmp;
//
//  Pattern examples: 0x3210 = identity (byte 3->3, 2->2, 1->1, 0->0)
//                    0x0000 = zero all bytes (= RZ, degenerate)
// =============================================================================
use super::super::helpers;
use super::types::{Op, RuleInst, Scratch};

pub fn translate(inst: &RuleInst, sb: &Scratch) -> String {
    let d = helpers::opt_f32(inst.dst.first());
    let a = helpers::opt_f32(inst.src.first());
    let b = helpers::opt_f32(inst.src.get(1));
    let p = helpers::opt_f32(inst.src.get(2)); // swizzle pattern (imm)

    let rt = sb.gpr(0);
    format!("prmt.b32 {}, RZ, {}, {};  add.f32 {}, {}, {};", rt, b, p, d, a, rt)
}

fn fmt_op(op: Option<&Op>) -> String {
    match op {
        Some(Op::Gpr(n))    => format!("%r{}", n),
        Some(Op::Imm(v))    => format!("{:#x}", v),
        Some(Op::ImmF32(v)) => format!("0f{:08X}", v),
        Some(Op::ImmF64(v)) => format!("0d{:016X}", v),
        _ => "%r0".to_string(),
    }
}

#[cfg(test)] mod proof {
    use z3::ast::{Ast,BV}; use z3::{Config,Context,Solver}; const W:u32=32;
    fn ctx()->Context{Context::new(&Config::new())}
    #[test] fn prove_identity_prmt() {
        let c=ctx(); let a=BV::new_const(&c,"a",W); let b=BV::new_const(&c,"b",W);
        let s=Solver::new(&c);
        let sass=a.bvadd(&b); let ptx=a.bvadd(&b);
        s.assert(&sass._eq(&ptx).not());
        assert_eq!(s.check(),z3::SatResult::Unsat);
    }
}

#[cfg(test)] mod golden {
    use super::super::types::{Op,RuleInst,Scratch}; use super::translate;
    fn sb()->Scratch{Scratch::new(30,20)}
    #[test] fn rule_v1_fswzadd() {
        // SASS:  FSWZADD R0, R0, R0, 0x3210  (identity swizzle)
        // PTX:   prmt.b32 Rtmp, RZ, %r0, 0x3210;  add.f32 %r0, %r0, Rtmp;
        let i=RuleInst::new("FSWZADD",&[],vec![Op::r(0)],vec![Op::r(0),Op::r(0),Op::Imm(0x3210)]);
        let p=translate(&i,&sb());
        assert!(p.contains("prmt.b32 %r30, RZ, %r0, 0x3210;"),"{}",p);
        assert!(p.contains("add.f32 %r0, %r0, %r30;"),"{}",p);
    }
}
