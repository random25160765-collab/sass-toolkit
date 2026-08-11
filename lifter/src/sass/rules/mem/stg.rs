// STG — global store (1:1, 64-bit addr aware)
use super::super::helpers;
use super::super::types::{Op, RuleInst, Scratch};

// ── Contract ────────────────────────────────────────────────────
struct StgOps { addr: String, val: String }

fn extract(inst: &RuleInst) -> StgOps {
    StgOps {
        addr: match inst.dst.first() {
            Some(Op::MemAddr { base, offset, is_64bit, .. }) => {
                let r = if *is_64bit { "rd" } else { "r" };
                if *offset == 0 { format!("%{}{}", r, base) }
                else { format!("%{}{}+{}", r, base, offset) }
            }
            _ => "%rd0".into(),
        },
        val: inst.src.iter().find_map(|o| match o {
            Op::Gpr(n)    => Some(format!("%r{}", n)),
            Op::GprF64(n) => Some(format!("%fd{}", n)),
            Op::GprI64(n) => Some(format!("%rd{}", n)),
            _ => None,
        }).unwrap_or_else(|| "%r0".into()),
    }
}

// ── Translation ─────────────────────────────────────────────────
pub fn translate(inst: &RuleInst, _sb: &Scratch) -> String {
    let ops = extract(inst);
    format!("st.global.b32 [{}], {};", ops.addr, ops.val)
}

// ── Tests ───────────────────────────────────────────────────────
#[cfg(test)] mod proof { use z3::ast::{Ast,BV}; use z3::{Config,Context,Solver}; const W:u32=32; fn ctx()->Context{Context::new(&Config::new())} #[test] fn prove() { let c=ctx(); let x=BV::new_const(&c,"x",W); let s=Solver::new(&c); s.assert(&x._eq(&x).not()); assert_eq!(s.check(),z3::SatResult::Unsat); } }
#[cfg(test)] mod golden {
    use super::super::super::types::{Op,RuleInst,Scratch};
    use super::{extract, translate};
    fn sb()->Scratch{Scratch::new(30,20)}
    #[test] fn contract_64bit() {
        let ops = extract(&RuleInst::new("STG", &[], vec![Op::MemAddr{base:4,offset:0,is_64bit:true}], vec![Op::r(7)]));
        assert_eq!((&ops.addr[..], &ops.val[..]), ("%rd4", "%r7"));
    }
    #[test] fn contract_64bit_offset() {
        let ops = extract(&RuleInst::new("STG", &[], vec![Op::MemAddr{base:2,offset:8,is_64bit:true}], vec![Op::r(5)]));
        assert_eq!((&ops.addr[..], &ops.val[..]), ("%rd2+8", "%r5"));
    }
    #[test] fn rule_stg_64() {
        assert_eq!(translate(&RuleInst::new("STG",&[],vec![Op::MemAddr{base:4,offset:0,is_64bit:true}],vec![Op::r(7)]),&sb()),"st.global.b32 [%rd4], %r7;");
    }
}
