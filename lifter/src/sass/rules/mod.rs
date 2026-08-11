// =============================================================================
//  rules/ -- SASS -> PTX Translation Rules
//
//  Each opcode has one .rs file in its category subdirectory.
//  Every file exports `pub fn translate(inst: &RuleInst, sb: &Scratch) -> String`.
//
//  ═══════════════════════════════════════════════════════════════════════════
//  CATALOG -- 11 categories, 127 files
//  ═══════════════════════════════════════════════════════════════════════════
//
//  arith/ (16)    Integer + Float arithmetic
//    iadd3            3-input integer add               ✓ 1:1
//    imad             integer multiply-add               ✓ 1:1
//    lea              load effective address             ✓ 1:1
//    iabs             integer absolute                   ✓ 1:1
//    fadd             float add                          ✓ 1:1
//    fmul             float multiply                      ✓ 1:1
//    ffma             fused float multiply-add            ✓ 1:1
//    dfma             double FMA                         ✓ 1:1
//    dadd             double add                         ✓ 1:1
//    dmul             double multiply                     ✓ 1:1
//    fmnmx            float min/max                      ✓ 1:1
//    frnd             float round                        ✓ 1:1
//    fset             float set                          ✓ 1:1
//    fswzadd          float swizzle-add                  ✓ 1:1
//    fchk             float check                        ✗ IMPOSSIBLE
//    mufu             multi-function (rsqrt, rcp, ...)   ✓ decomposed
//
//  bit/ (14)      Bit manipulation + Logic + Packed
//    lop3            3-input Boolean LUT                 ✓ 1:1
//    shf             funnel shift                        ✓ 1:1
//    popc            population count                    ✓ 1:1
//    brev            bit reverse                         ✓ 1:1
//    flo / uflo      find leading one                   ✓ 1:1
//    bmov            bit move                            ✓ 1:1
//    bmsk            bit mask                            ✓ 1:1
//    prmt            byte permute                        ✓ 1:1
//    imnmx           integer min/max                     ✓ 1:1
//    fsel            float select                        ✓ 1:1
//    movm            matrix move                         ✓ 1:1
//    vabsdiff        vector abs diff (2-way)             ✓ 1:1
//    vabsdiff4       vector abs diff (4-way)             ✓ 1:1
//
//  mem/ (16)      Memory access
//    ldg / stg       global load/store                   ✓ 1:1
//    lds / sts       shared load/store                   ✓ 1:1
//    ldl / stl       local load/store                    ✓ 1:1
//    ld / st         generic load/store                  ✓ 1:1
//    ldc             load constant                       ✓ 1:1
//    ldsm            load shared matrix                  ✓ 1:1
//    ldgsts          load-global store-shared            ✓ 1:1
//    ldgdepbar       load-global dep-barrier             ✗ IMPOSSIBLE
//    atomg           global atomic                       ✓ 1:1
//    atom            shared atom (move)                  ✓ 1:1
//    atoms           shared atomic (add/cas/...)         ✓ 1:1
//    ldtram          load tensor RAM                     ✗ IMPOSSIBLE
//
//  ctrl/ (14)     Control flow
//    bra             branch                              ✓ 1:1
//    jmp             jump                                ✓ 1:1
//    call            call                                ✓ 1:1
//    ret             return                              ✓ 1:1
//    brx / brxu      indexed branch                      ✓ decomposed
//    exit            exit thread                         ✓ 1:1
//    jmx / jmxu      indexed jump                        ✗ IMPOSSIBLE
//    bssy / bsync    barrier set-sync                    ✗ IMPOSSIBLE
//    nop             no-op                               ✓ 1:1
//    break / yield   break / yield                       ✗ IMPOSSIBLE
//
//  sync/ (8)      Synchronization + Warp
//    bar             barrier                             ✗ IMPOSSIBLE
//    membar          memory barrier                      ✗ IMPOSSIBLE
//    depbar          dependency barrier                  ✗ IMPOSSIBLE
//    warpsync        warp synchronize                    ✗ IMPOSSIBLE
//    vote            warp vote (all/any/ballot)          ✓ 1:1
//    shfl            warp shuffle                        ✓ 1:1
//    red             reduction                           -> upstream
//    redux           uniform reduction                   ✗ IMPOSSIBLE
//
//  convert/ (10)  Type conversion
//    f2i             float->integer                       ✓ 1:1
//    i2f             integer->float                       ✓ 1:1
//    f2f             float->float (width change)          ✓ decomposed
//    i2i             integer->integer (width change)      ✓ 1:1
//    f2fp            float->float-point (packed)          ✗ IMPOSSIBLE
//    i2fp            integer->float-point (packed)        ✗ IMPOSSIBLE
//    f2ip            float->integer-packed                ✓ decomposed
//    i2ip            integer->integer-packed              ✓ 1:1
//    sgxt            sign extend                         ✓ decomposed
//    lepc            load effective PC                   -> upstream
//
//  pred/ (10)     Predicate + Comparison + Select
//    isetp           integer set-predicate               ✓ decomposed
//    fsetp           float set-predicate                 ✓ decomposed
//    dsetp           double set-predicate                ✓ decomposed
//    hsetp2          half set-pred (packed)              ✓ decomposed
//    plop3           predicate LOP3 (LUT)                ✓ decomposed
//    uisetp          uniform set-predicate               ✓ decomposed
//    hset2           half-pred set-pred (packed)         ✓ decomposed
//    hmnmx2          half min/max                        ✓ 1:1
//    sel             select (GPR -> GPR)                  ✓ 1:1
//    mov             move (GPR -> GPR)                    ✓ 1:1
//
//  hf/ (3)        Half-Precision (FP16)
//    hadd2           half add                            ✓ 1:1
//    hmul2           half multiply                       ✓ 1:1
//    hfma2           half fused multiply-add             ✓ 1:1
//
//  tensor/ (5)    Tensor core
//    hmma            FP16 tensor                         ✓ 1:1
//    bmma            BF16 tensor                         ✓ 1:1
//    dmma            FP64 tensor                         ✓ 1:1
//    qmma            quantized tensor                    ✓ 1:1
//    imma            INT tensor                          ✓ 1:1
//
//  uniform/ (13)  Uniform register variants
//    uiadd3          uniform IADD3                       ✓ 1:1
//    uimad           uniform IMAD                        ✓ 1:1
//    ulea            uniform LEA                         ✓ 1:1
//    ulop3           uniform LOP3                        ✓ 1:1
//    uf2fp           uniform float->float-point           ✗ IMPOSSIBLE
//    uplop3          uniform predicate LOP3              ✓ decomposed
//    up2ur           uniform pred->UR                     ✓ 1:1
//    uclea           uniform complex LEA                 ✓ 1:1
//    ubmsk           uniform BMSK                        ✓ 1:1
//    uprmt           uniform PRMT                        ✓ 1:1
//    usgxt           uniform SGXT                        ✓ decomposed
//    ushf            uniform SHF                         ✓ 1:1
//    uldc            uniform LDC (load constant)         -> upstream (cbank)
//
//  sys/ (20)      System regs + Hardware + Misc
//    b2r / r2p / p2r  barrier/predicate ↔ register      ✓ 1:1
//    cs2r / s2r / s2ur  special register -> GPR/UR        -> upstream (SR table)
//    getlmembase    get local memory base                -> upstream (SR table)
//    rpcmov         RPC register move                    ✗ IMPOSSIBLE
//    setctaid       set CTA ID                           ✗ IMPOSSIBLE
//    umov / r2ur / ur2up  uniform ↔ predicate            ✓ 1:1
//    cctl / cctll   cache control                        ✗ IMPOSSIBLE
//    errbar         error barrier                        ✗ IMPOSSIBLE
//    idp            interpolated double precision (gfx)  ✗ IMPOSSIBLE
//    isberd         scalar complex (BERD)                 ✓ 1:1
//    csmtest        CSM test (debug)                     ✗ IMPOSSIBLE
//    footprint      texture footprint query              ✗ IMPOSSIBLE
//    match          legacy warp vote (alias VOTE)         ✗ IMPOSSIBLE
//
//  ═══════════════════════════════════════════════════════════════════════════
//  DISPOSITION LEGEND
//  ═══════════════════════════════════════════════════════════════════════════
//    ✓ 1:1        -- SASS->PTX direct mapping, no decomposition
//    ✓ decomposed -- multi-instruction decomposition, Z3 proven
//    ✗ IMPOSSIBLE -- no PTX equivalent (barrier/hw/internal)
//    -> upstream   -- depends on pending infrastructure (cbank, SR table, desc)
//
//  ═══════════════════════════════════════════════════════════════════════════
//  ISA COVERAGE
//  ═══════════════════════════════════════════════════════════════════════════
//    SM89 ISA manual:      151 opcodes
//    Handled (rules):      125 files (incl. covered by base rules)
//    Not handled (excl):    26 (texture/surface/graphics -- out of scope)
//
//    All CUDA compute opcodes are covered.
//    Texture/surface/graphics ops are excluded by design.
// =============================================================================

pub mod types;
pub mod helpers;

pub mod arith;
pub mod bit;
pub mod convert;
pub mod ctrl;
pub mod hf;
pub mod mem;
pub mod pred;
pub mod sync;
pub mod sys;
pub mod tensor;
pub mod uniform;

// Re-export all opcode modules at `rules::<opcode>` for flat lifter dispatch.
pub use self::arith::*;
pub use self::bit::*;
pub use self::convert::*;
pub use self::ctrl::*;
pub use self::hf::*;
pub use self::mem::*;
pub use self::pred::*;
pub use self::sync::*;
pub use self::sys::*;
pub use self::tensor::*;
pub use self::uniform::*;
