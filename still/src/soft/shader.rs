//! Running wgw's shaders on the CPU. A shader's entry point (naga's IR of the WGSL the page's GLSL is generated
//! from) is compiled once into a flat list of instructions over an array of 32-bit registers, with function calls
//! inlined; running it is a loop over those instructions, with no allocation. Values are laid out flat: a vector's
//! components, a matrix's columns, a struct's members and an array's elements in consecutive registers, as raw bits
//! (f32, i32, u32, or 0/1 for booleans). It covers what shaders for drawing need: arithmetic on scalars, vectors
//! and matrices, structs and arrays, local variables, function calls, branches and loops, texture sampling, and
//! derivatives of the fragment shader's inputs.

use naga::proc::TypeResolution;
use naga::valid::{Capabilities, FunctionInfo, ModuleInfo, ValidationFlags, Validator};
use naga::{
    BinaryOperator as B, Binding, BuiltIn, Expression as E, Handle, MathFunction as M, Module, ScalarKind, ShaderStage,
    Statement as S, TypeInner, UnaryOperator as U,
};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    F,
    I,
    U,
    B,
}

const NONE: usize = usize::MAX;

#[derive(Clone, Debug)]
enum Op {
    Copy { dst: usize, src: usize, n: usize },
    Bin { op: B, k: Kind, dst: usize, a: usize, b: usize, n: usize, sa: bool, sb: bool },
    Un { op: U, k: Kind, dst: usize, a: usize, n: usize },
    Math { f: M, k: Kind, dst: usize, a: usize, b: usize, c: usize, n: usize, sb: bool, sc: bool },
    MatVec { dst: usize, m: usize, v: usize, cols: usize, rows: usize },
    VecMat { dst: usize, v: usize, m: usize, cols: usize, rows: usize },
    MatMat { dst: usize, a: usize, b: usize, rows: usize, inner: usize, cols: usize },
    Select { dst: usize, c: usize, a: usize, b: usize, n: usize, sc: bool },
    Cast { dst: usize, src: usize, n: usize, from: Kind, to: Kind, convert: bool },
    Swizzle { dst: usize, src: usize, pattern: [u8; 4], n: usize },
    Splat { dst: usize, src: usize, n: usize },
    Sample { dst: usize, coord: usize },
    Derivative { dst: usize, input: usize, n: usize, y: bool },
    Addr { dst: usize, base: usize, index: usize, stride: usize, add: usize },
    LoadDyn { dst: usize, addr: usize, n: usize },
    StoreDyn { addr: usize, src: usize, n: usize },
    JumpIfZero { cond: usize, to: usize },
    JumpIfNonZero { cond: usize, to: usize },
    Jump { to: usize },
    Kill,
}

/// Where a value is, at compile time.
#[derive(Clone, Copy, Debug)]
enum Loc {
    Val(usize),
    Ptr(usize, Off),
    Texture,
    Sampler,
}

#[derive(Clone, Copy, Debug)]
enum Off {
    Static(usize),
    /// A register holding the absolute register index (an address).
    Dyn(usize),
}

/// What a compiled entry point reads and writes, beyond its registers.
#[derive(Clone, Debug)]
pub enum Io {
    Attribute { location: u32, slot: usize, n: usize },
    VertexIndex(usize),
    InstanceIndex(usize),
    Position(usize),
    Location { location: u32, slot: usize, n: usize, kind: Kind, flat: bool },
}

/// A compiled entry point.
pub struct Program {
    ops: Vec<Op>,
    /// The registers as they start: constants in place.
    template: Vec<u32>,
    /// Its inputs and outputs: for a vertex shader, attributes and indices in, position and varyings out; for a
    /// fragment shader, position and varyings in, a color out.
    pub inputs: Vec<Io>,
    pub outputs: Vec<Io>,
    /// The uniform registers, from bytes of the uniform buffer: (register, byte offset).
    uniforms: Vec<(usize, usize)>,
    /// Whether it takes derivatives of its inputs (so a fragment needs its inputs' gradients).
    pub derivatives: bool,
    /// Where the inputs start, so gradients line up with them.
    pub input_base: usize,
    pub input_len: usize,
}

/// A texture as sampled: RGBA in [0, 1], `width` by `height`, rows from the bottom; linear filtering, clamped.
pub struct Image<'a> {
    pub width: usize,
    pub height: usize,
    pub texels: &'a [[f32; 4]],
}

impl Image<'_> {
    fn sample(&self, u: f32, v: f32) -> [f32; 4] {
        if self.width == 0 || self.height == 0 {
            return [0.0; 4];
        }
        let at = |x: i64, y: i64| {
            let x = x.clamp(0, self.width as i64 - 1) as usize;
            let y = y.clamp(0, self.height as i64 - 1) as usize;
            self.texels[y * self.width + x]
        };
        let (x, y) = (u * self.width as f32 - 0.5, v * self.height as f32 - 0.5);
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        let (a, b, c, d) = (at(x0, y0), at(x0 + 1, y0), at(x0, y0 + 1), at(x0 + 1, y0 + 1));
        std::array::from_fn(|i| {
            let top = a[i] * (1.0 - fx) + b[i] * fx;
            let bottom = c[i] * (1.0 - fx) + d[i] * fx;
            top * (1.0 - fy) + bottom * fy
        })
    }
}

pub fn validate(module: &Module) -> Result<ModuleInfo, String> {
    Validator::new(ValidationFlags::all(), Capabilities::all()).validate(module).map_err(|e| format!("{e:?}"))
}

fn kind(s: naga::Scalar) -> Kind {
    match s.kind {
        ScalarKind::Float | ScalarKind::AbstractFloat => Kind::F,
        ScalarKind::Sint | ScalarKind::AbstractInt => Kind::I,
        ScalarKind::Uint => Kind::U,
        ScalarKind::Bool => Kind::B,
    }
}

/// How many registers a value of this type takes.
fn size(module: &Module, inner: &TypeInner) -> usize {
    match inner {
        TypeInner::Scalar(_) => 1,
        TypeInner::Vector { size, .. } => *size as usize,
        TypeInner::Matrix { columns, rows, .. } => *columns as usize * *rows as usize,
        TypeInner::Array { base, size: naga::ArraySize::Constant(n), .. } => n.get() as usize * size(module, &module.types[*base].inner),
        TypeInner::Struct { members, .. } => members.iter().map(|m| size(module, &module.types[m.ty].inner)).sum(),
        TypeInner::Pointer { base, .. } => size(module, &module.types[*base].inner),
        TypeInner::ValuePointer { size: Some(s), .. } => *s as usize,
        TypeInner::ValuePointer { size: None, .. } => 1,
        _ => 0,
    }
}

/// The kind of a value's components (floats for matrices).
fn kind_of(module: &Module, inner: &TypeInner) -> Kind {
    match inner {
        TypeInner::Scalar(s) | TypeInner::Vector { scalar: s, .. } | TypeInner::Matrix { scalar: s, .. } => kind(*s),
        TypeInner::Pointer { base, .. } => kind_of(module, &module.types[*base].inner),
        TypeInner::ValuePointer { scalar, .. } => kind(*scalar),
        _ => Kind::F,
    }
}

/// The register offset of element `i` within a value of this type, and the element's type.
fn element(module: &Module, inner: &TypeInner, i: usize) -> (usize, TypeInner) {
    match inner {
        TypeInner::Vector { scalar, .. } => (i, TypeInner::Scalar(*scalar)),
        TypeInner::Matrix { rows, scalar, .. } => (i * *rows as usize, TypeInner::Vector { size: *rows, scalar: *scalar }),
        TypeInner::Array { base, .. } => {
            let b = &module.types[*base].inner;
            (i * size(module, b), b.clone())
        }
        TypeInner::Struct { members, .. } => {
            let offset = members[..i].iter().map(|m| size(module, &module.types[m.ty].inner)).sum();
            (offset, module.types[members[i].ty].inner.clone())
        }
        TypeInner::Pointer { base, .. } => element(module, &module.types[*base].inner, i),
        TypeInner::ValuePointer { scalar, .. } => (i, TypeInner::Scalar(*scalar)),
        other => panic!("can't index {other:?}"),
    }
}

struct Compiler<'m> {
    module: &'m Module,
    info: &'m ModuleInfo,
    ops: Vec<Op>,
    template: Vec<u32>,
    uniforms: Vec<(usize, usize)>,
    globals: Vec<Option<Loc>>,
    inputs: Vec<Io>,
    outputs: Vec<Io>,
    input_range: (usize, usize),
    derivatives: bool,
}

struct Func<'m> {
    function: &'m naga::Function,
    info: &'m FunctionInfo,
    args: Vec<Loc>,
    exprs: Vec<Option<Loc>>,
    locals: Vec<usize>,
    /// For an inlined call: where its result goes, and the jumps to patch to its end.
    ret: Option<usize>,
    exits: Vec<usize>,
    /// Loops: (continue target, break jumps to patch).
    loops: Vec<(usize, Vec<usize>)>,
    entry: bool,
}

impl<'m> Compiler<'m> {
    fn alloc(&mut self, n: usize) -> usize {
        let at = self.template.len();
        self.template.resize(at + n.max(1), 0);
        at
    }

    fn constant(&mut self, bits: &[u32]) -> usize {
        let at = self.alloc(bits.len());
        self.template[at..at + bits.len()].copy_from_slice(bits);
        at
    }

    fn ty(&self, f: &Func, h: Handle<naga::Expression>) -> TypeInner {
        match &f.info[h].ty {
            TypeResolution::Handle(t) => self.module.types[*t].inner.clone(),
            TypeResolution::Value(v) => v.clone(),
        }
    }

    fn emit(&mut self, op: Op) -> usize {
        self.ops.push(op);
        self.ops.len() - 1
    }

    fn patch(&mut self, at: usize) {
        let to = self.ops.len();
        match &mut self.ops[at] {
            Op::JumpIfZero { to: t, .. } | Op::JumpIfNonZero { to: t, .. } | Op::Jump { to: t } => *t = to,
            _ => unreachable!(),
        }
    }

    /// A value's register (copying a pointer's target out, as a load does).
    fn value(&mut self, f: &mut Func<'m>, h: Handle<naga::Expression>) -> usize {
        match self.expr(f, h) {
            Loc::Val(r) => r,
            other => panic!("expected a value, got {other:?}"),
        }
    }

    fn expr(&mut self, f: &mut Func<'m>, h: Handle<naga::Expression>) -> Loc {
        if let Some(loc) = f.exprs[h.index()] {
            return loc;
        }
        let loc = self.compute(f, h);
        f.exprs[h.index()] = Some(loc);
        loc
    }

    fn compute(&mut self, f: &mut Func<'m>, h: Handle<naga::Expression>) -> Loc {
        let module = self.module;
        let ty = self.ty(f, h);
        let n = size(module, &ty);
        let k = kind_of(module, &ty);
        match &f.function.expressions[h] {
            E::Literal(l) => Loc::Val(self.literal(l)),
            E::Constant(c) => Loc::Val(self.global_expression(module.constants[*c].init)),
            E::ZeroValue(_) => Loc::Val(self.alloc(n)),
            E::Compose { components, .. } => {
                let dst = self.alloc(n);
                let mut at = dst;
                for &c in components {
                    let cn = size(module, &self.ty(f, c));
                    let src = self.value(f, c);
                    self.emit(Op::Copy { dst: at, src, n: cn });
                    at += cn;
                }
                Loc::Val(dst)
            }
            E::AccessIndex { base, index } => {
                let base_ty = self.ty(f, *base);
                let (offset, _) = element(module, &base_ty, *index as usize);
                match self.expr(f, *base) {
                    Loc::Val(r) => Loc::Val(r + offset),
                    Loc::Ptr(b, Off::Static(o)) => Loc::Ptr(b, Off::Static(o + offset)),
                    Loc::Ptr(b, Off::Dyn(a)) => {
                        let dst = self.alloc(1);
                        let one = self.constant(&[1]);
                        self.emit(Op::Addr { dst, base: 0, index: one, stride: 0, add: offset });
                        self.emit(Op::Bin { op: B::Add, k: Kind::U, dst, a, b: dst, n: 1, sa: false, sb: false });
                        Loc::Ptr(b, Off::Dyn(dst))
                    }
                    other => panic!("can't index {other:?}"),
                }
            }
            E::Access { base, index } => {
                let base_ty = self.ty(f, *base);
                let (stride, _) = element(module, &base_ty, 1);
                let i = self.value(f, *index);
                match self.expr(f, *base) {
                    Loc::Val(r) => {
                        let addr = self.alloc(1);
                        self.emit(Op::Addr { dst: addr, base: r, index: i, stride, add: 0 });
                        let dst = self.alloc(n);
                        self.emit(Op::LoadDyn { dst, addr, n });
                        Loc::Val(dst)
                    }
                    Loc::Ptr(b, Off::Static(o)) => {
                        let addr = self.alloc(1);
                        self.emit(Op::Addr { dst: addr, base: b + o, index: i, stride, add: 0 });
                        Loc::Ptr(b, Off::Dyn(addr))
                    }
                    Loc::Ptr(b, Off::Dyn(a)) => {
                        let addr = self.alloc(1);
                        self.emit(Op::Addr { dst: addr, base: 0, index: i, stride, add: 0 });
                        self.emit(Op::Bin { op: B::Add, k: Kind::U, dst: addr, a, b: addr, n: 1, sa: false, sb: false });
                        Loc::Ptr(b, Off::Dyn(addr))
                    }
                    other => panic!("can't index {other:?}"),
                }
            }
            E::Splat { value, .. } => {
                let src = self.value(f, *value);
                let dst = self.alloc(n);
                self.emit(Op::Splat { dst, src, n });
                Loc::Val(dst)
            }
            E::Swizzle { vector, pattern, .. } => {
                let src = self.value(f, *vector);
                let dst = self.alloc(n);
                self.emit(Op::Swizzle { dst, src, pattern: pattern.map(|p| p as u8), n });
                Loc::Val(dst)
            }
            E::FunctionArgument(i) => f.args[*i as usize],
            E::GlobalVariable(g) => self.globals[g.index()].expect("a global without a place"),
            E::LocalVariable(l) => Loc::Ptr(f.locals[l.index()], Off::Static(0)),
            E::Load { pointer } => match self.expr(f, *pointer) {
                Loc::Ptr(b, Off::Static(o)) => {
                    let dst = self.alloc(n);
                    self.emit(Op::Copy { dst, src: b + o, n });
                    Loc::Val(dst)
                }
                Loc::Ptr(_, Off::Dyn(addr)) => {
                    let dst = self.alloc(n);
                    self.emit(Op::LoadDyn { dst, addr, n });
                    Loc::Val(dst)
                }
                other => other,
            },
            E::ImageSample { coordinate, .. } => {
                let coord = self.value(f, *coordinate);
                let dst = self.alloc(4);
                self.emit(Op::Sample { dst, coord });
                Loc::Val(dst)
            }
            E::Unary { op, expr } => {
                let a = self.value(f, *expr);
                let dst = self.alloc(n);
                self.emit(Op::Un { op: *op, k, dst, a, n });
                Loc::Val(dst)
            }
            E::Binary { op, left, right } => {
                let (lt, rt) = (self.ty(f, *left), self.ty(f, *right));
                let (a, b) = (self.value(f, *left), self.value(f, *right));
                let dst = self.alloc(n);
                match (&lt, &rt, op) {
                    (TypeInner::Matrix { columns, rows, .. }, TypeInner::Vector { .. }, B::Multiply) => {
                        self.emit(Op::MatVec { dst, m: a, v: b, cols: *columns as usize, rows: *rows as usize });
                    }
                    (TypeInner::Vector { .. }, TypeInner::Matrix { columns, rows, .. }, B::Multiply) => {
                        self.emit(Op::VecMat { dst, v: a, m: b, cols: *columns as usize, rows: *rows as usize });
                    }
                    (TypeInner::Matrix { columns: ac, rows: ar, .. }, TypeInner::Matrix { columns: bc, .. }, B::Multiply) => {
                        self.emit(Op::MatMat { dst, a, b, rows: *ar as usize, inner: *ac as usize, cols: *bc as usize });
                    }
                    _ => {
                        let (la, lb) = (size(module, &lt), size(module, &rt));
                        let k = kind_of(module, &lt);
                        self.emit(Op::Bin { op: *op, k, dst, a, b, n, sa: la == 1 && n > 1, sb: lb == 1 && n > 1 });
                    }
                }
                Loc::Val(dst)
            }
            E::Select { condition, accept, reject } => {
                let sc = size(module, &self.ty(f, *condition)) == 1;
                let (c, a, b) = (self.value(f, *condition), self.value(f, *accept), self.value(f, *reject));
                let dst = self.alloc(n);
                self.emit(Op::Select { dst, c, a, b, n, sc });
                Loc::Val(dst)
            }
            E::Derivative { axis, expr, .. } => {
                // Of the fragment shader's inputs (interpolated varyings), whose gradients the rasterizer knows.
                let Loc::Val(input) = self.expr(f, *expr) else { panic!("a derivative of a pointer") };
                let (lo, hi) = self.input_range;
                assert!(input >= lo && input + n <= hi, "the still takes derivatives of the fragment shader's inputs only");
                self.derivatives = true;
                let dst = self.alloc(n);
                self.emit(Op::Derivative { dst, input, n, y: matches!(axis, naga::DerivativeAxis::Y) });
                Loc::Val(dst)
            }
            E::Relational { fun, argument } => {
                let an = size(module, &self.ty(f, *argument));
                let a = self.value(f, *argument);
                let dst = self.alloc(1);
                let fun = match fun {
                    naga::RelationalFunction::All => M::Min,
                    _ => M::Max, // any; isnan and isinf aren't used by wgw's shaders
                };
                self.emit(Op::Math { f: fun, k: Kind::U, dst, a, b: NONE, c: NONE, n: an, sb: false, sc: true });
                Loc::Val(dst)
            }
            E::Math { fun, arg, arg1, arg2, .. } => {
                let at = self.ty(f, *arg);
                let an = size(module, &at);
                let ak = kind_of(module, &at);
                let a = self.value(f, *arg);
                let (mut b, mut c, mut sb, mut sc) = (NONE, NONE, false, false);
                if let Some(x) = arg1 {
                    sb = size(module, &self.ty(f, *x)) == 1 && an > 1;
                    b = self.value(f, *x);
                }
                if let Some(x) = arg2 {
                    sc = size(module, &self.ty(f, *x)) == 1 && an > 1;
                    c = self.value(f, *x);
                }
                let dst = self.alloc(n.max(an));
                if *fun == M::Cross {
                    self.emit(Op::Math { f: M::Cross, k: ak, dst, a, b, c, n: 3, sb, sc });
                } else {
                    self.emit(Op::Math { f: *fun, k: ak, dst, a, b, c, n: an, sb, sc });
                }
                Loc::Val(dst)
            }
            E::As { expr, kind: to, convert } => {
                let from = kind_of(module, &self.ty(f, *expr));
                let src = self.value(f, *expr);
                let dst = self.alloc(n);
                let to = kind(naga::Scalar { kind: *to, width: 4 });
                self.emit(Op::Cast { dst, src, n, from, to, convert: convert.is_some() });
                Loc::Val(dst)
            }
            E::CallResult(_) => panic!("a call's result before the call"),
            other => panic!("the still can't compile {other:?}"),
        }
    }

    fn literal(&mut self, l: &naga::Literal) -> usize {
        use naga::Literal as L;
        let bits = match *l {
            L::F32(x) => x.to_bits(),
            L::F64(x) | L::AbstractFloat(x) => (x as f32).to_bits(),
            L::U32(x) => x,
            L::I32(x) => x as u32,
            L::U64(x) => x as u32,
            L::I64(x) | L::AbstractInt(x) => x as i32 as u32,
            L::Bool(b) => b as u32,
            _ => panic!("unsupported literal {l:?}"),
        };
        self.constant(&[bits])
    }

    /// A constant's registers, from the module's global expressions.
    fn global_expression(&mut self, h: Handle<naga::Expression>) -> usize {
        let module = self.module;
        let bits = self.constant_bits(h);
        let _ = module;
        self.constant(&bits)
    }

    fn constant_bits(&self, h: Handle<naga::Expression>) -> Vec<u32> {
        let module = self.module;
        use naga::Literal as L;
        match &module.global_expressions[h] {
            E::Literal(l) => vec![match *l {
                L::F32(x) => x.to_bits(),
                L::F64(x) | L::AbstractFloat(x) => (x as f32).to_bits(),
                L::U32(x) => x,
                L::I32(x) => x as u32,
                L::I64(x) | L::AbstractInt(x) => x as i32 as u32,
                L::Bool(b) => b as u32,
                _ => 0,
            }],
            E::ZeroValue(ty) => vec![0; size(module, &module.types[*ty].inner)],
            E::Compose { components, .. } => components.iter().flat_map(|&c| self.constant_bits(c)).collect(),
            E::Splat { size: s, value } => {
                let v = self.constant_bits(*value);
                (0..*s as usize).map(|_| v[0]).collect()
            }
            E::Constant(c) => self.constant_bits(module.constants[*c].init),
            other => panic!("unsupported constant expression {other:?}"),
        }
    }

    fn block(&mut self, f: &mut Func<'m>, block: &'m naga::Block) {
        for statement in block.iter() {
            match statement {
                S::Emit(range) => {
                    for h in range.clone() {
                        self.expr(f, h);
                    }
                }
                S::Block(b) => self.block(f, b),
                S::If { condition, accept, reject } => {
                    let cond = self.value(f, *condition);
                    let skip = self.emit(Op::JumpIfZero { cond, to: 0 });
                    self.block(f, accept);
                    let end = self.emit(Op::Jump { to: 0 });
                    self.patch(skip);
                    self.block(f, reject);
                    self.patch(end);
                }
                S::Return { value } => {
                    if let Some(v) = value {
                        let src = self.value(f, *v);
                        let n = size(self.module, &self.ty(f, *v));
                        if f.entry {
                            self.write_outputs(f, src);
                        } else if let Some(dst) = f.ret {
                            self.emit(Op::Copy { dst, src, n });
                        }
                    }
                    let exit = self.emit(Op::Jump { to: 0 });
                    f.exits.push(exit);
                }
                S::Kill => {
                    self.emit(Op::Kill);
                }
                S::Store { pointer, value } => {
                    let n = size(self.module, &self.ty(f, *value));
                    let src = self.value(f, *value);
                    match self.expr(f, *pointer) {
                        Loc::Ptr(b, Off::Static(o)) => {
                            self.emit(Op::Copy { dst: b + o, src, n });
                        }
                        Loc::Ptr(_, Off::Dyn(addr)) => {
                            self.emit(Op::StoreDyn { addr, src, n });
                        }
                        other => panic!("a store to {other:?}"),
                    }
                }
                S::Call { function, arguments, result } => {
                    let args: Vec<Loc> = arguments.iter().map(|&a| self.expr(f, a)).collect();
                    let callee = &self.module.functions[*function];
                    let ret = result.map(|r| {
                        let n = size(self.module, &self.ty(f, r));
                        let slot = self.alloc(n);
                        f.exprs[r.index()] = Some(Loc::Val(slot));
                        slot
                    });
                    self.function(callee, &self.info[*function], args, ret, false);
                }
                S::Loop { body, continuing, break_if } => {
                    let start = self.ops.len();
                    f.loops.push((usize::MAX, Vec::new()));
                    self.block(f, body);
                    let cont = self.ops.len();
                    f.loops.last_mut().unwrap().0 = cont;
                    self.block(f, continuing);
                    if let Some(b) = break_if {
                        let cond = self.value(f, *b);
                        let out = self.emit(Op::JumpIfNonZero { cond, to: 0 });
                        f.loops.last_mut().unwrap().1.push(out);
                    }
                    self.emit(Op::Jump { to: start });
                    let (cont, breaks) = f.loops.pop().unwrap();
                    for b in breaks {
                        self.patch(b);
                    }
                    // Continues were emitted as jumps to usize::MAX; point them at the continuing block.
                    for op in &mut self.ops[start..] {
                        if let Op::Jump { to } = op {
                            if *to == usize::MAX - 1 {
                                *to = cont;
                            }
                        }
                    }
                }
                S::Break => {
                    let at = self.emit(Op::Jump { to: 0 });
                    f.loops.last_mut().expect("break outside a loop").1.push(at);
                }
                S::Continue => {
                    self.emit(Op::Jump { to: usize::MAX - 1 });
                }
                other => panic!("the still can't compile {other:?}"),
            }
        }
    }

    /// Compile a function inline: its arguments are these places, its result goes to `ret`.
    fn function(&mut self, function: &'m naga::Function, info: &'m FunctionInfo, args: Vec<Loc>, ret: Option<usize>, entry: bool) {
        let mut f = Func {
            function,
            info,
            args,
            exprs: vec![None; function.expressions.len()],
            locals: Vec::new(),
            ret,
            exits: Vec::new(),
            loops: Vec::new(),
            entry,
        };
        for (_, local) in function.local_variables.iter() {
            let n = size(self.module, &self.module.types[local.ty].inner);
            let slot = self.alloc(n);
            f.locals.push(slot);
        }
        // Locals start as their initializers, or zero, each time the function runs.
        for (h, local) in function.local_variables.iter() {
            let n = size(self.module, &self.module.types[local.ty].inner);
            let src = match local.init {
                Some(init) => self.value(&mut f, init),
                None => self.alloc(n),
            };
            let dst = f.locals[h.index()];
            self.emit(Op::Copy { dst, src, n });
        }
        self.block(&mut f, &function.body);
        for exit in std::mem::take(&mut f.exits) {
            self.patch(exit);
        }
    }

    /// An entry point's result to its outputs.
    fn write_outputs(&mut self, f: &Func, src: usize) {
        let result = f.function.result.as_ref().unwrap();
        let module = self.module;
        let mut outs = Vec::new();
        match (&result.binding, &module.types[result.ty].inner) {
            (Some(binding), inner) => outs.push((binding.clone(), 0, inner.clone())),
            (None, TypeInner::Struct { members, .. }) => {
                let mut at = 0;
                for m in members {
                    let inner = module.types[m.ty].inner.clone();
                    outs.push((m.binding.clone().unwrap(), at, inner.clone()));
                    at += size(module, &inner);
                }
            }
            _ => panic!("an entry point's result needs bindings"),
        }
        if self.outputs.is_empty() {
            for (binding, at, inner) in &outs {
                let n = size(module, inner);
                let slot = self.alloc(n);
                self.outputs.push(match binding {
                    Binding::BuiltIn(BuiltIn::Position { .. }) => Io::Position(slot),
                    Binding::Location { location, interpolation, .. } => {
                        let k = kind_of(module, inner);
                        Io::Location { location: *location, slot, n, kind: k, flat: *interpolation == Some(naga::Interpolation::Flat) || k != Kind::F }
                    }
                    other => panic!("unsupported output {other:?}"),
                });
                let _ = at;
            }
        }
        for (i, (_, at, inner)) in outs.iter().enumerate() {
            let n = size(module, inner);
            let dst = match &self.outputs[i] {
                Io::Position(s) => *s,
                Io::Location { slot, .. } => *slot,
                _ => unreachable!(),
            };
            self.ops.push(Op::Copy { dst, src: src + at, n });
        }
    }
}

/// Compile an entry point of the module.
pub fn compile(module: &Module, info: &ModuleInfo, entry: usize) -> Result<Program, String> {
    let ep = &module.entry_points[entry];
    let mut c = Compiler {
        module,
        info,
        ops: Vec::new(),
        template: vec![0; 1],
        uniforms: Vec::new(),
        globals: vec![None; module.global_variables.len()],
        inputs: Vec::new(),
        outputs: Vec::new(),
        input_range: (0, 0),
        derivatives: false,
    };
    // Globals: uniforms get registers, filled from the uniform buffer's bytes at each draw.
    for (h, g) in module.global_variables.iter() {
        let inner = &module.types[g.ty].inner;
        c.globals[h.index()] = Some(match (&g.space, inner) {
            (naga::AddressSpace::Uniform, _) => {
                let n = size(module, inner);
                let base = c.alloc(n);
                let mut fields = Vec::new();
                uniform_layout(module, g.ty, 0, base, &mut fields);
                c.uniforms.extend(fields);
                Loc::Ptr(base, Off::Static(0))
            }
            (_, TypeInner::Image { .. }) => Loc::Texture,
            (_, TypeInner::Sampler { .. }) => Loc::Sampler,
            _ => {
                let base = c.alloc(size(module, inner));
                Loc::Ptr(base, Off::Static(0))
            }
        });
    }
    // Inputs: each argument (or struct member) by its binding, in consecutive registers.
    let start = c.template.len();
    let mut args = Vec::new();
    for a in &ep.function.arguments {
        let inner = module.types[a.ty].inner.clone();
        let n = size(module, &inner);
        let slot = c.alloc(n);
        args.push(Loc::Val(slot));
        let bind = |binding: &Binding, slot: usize, inner: &TypeInner, c: &mut Compiler| {
            let n = size(module, inner);
            c.inputs.push(match (binding, ep.stage) {
                (Binding::BuiltIn(BuiltIn::VertexIndex), _) => Io::VertexIndex(slot),
                (Binding::BuiltIn(BuiltIn::InstanceIndex), _) => Io::InstanceIndex(slot),
                (Binding::BuiltIn(BuiltIn::Position { .. }), _) => Io::Position(slot),
                (Binding::Location { location, .. }, ShaderStage::Vertex) => Io::Attribute { location: *location, slot, n },
                (Binding::Location { location, interpolation, .. }, _) => {
                    let k = kind_of(module, inner);
                    Io::Location { location: *location, slot, n, kind: k, flat: *interpolation == Some(naga::Interpolation::Flat) || k != Kind::F }
                }
                (other, _) => panic!("unsupported input {other:?}"),
            });
        };
        match (&a.binding, &inner) {
            (Some(b), _) => bind(b, slot, &inner, &mut c),
            (None, TypeInner::Struct { members, .. }) => {
                let mut at = slot;
                for m in members {
                    let mi = module.types[m.ty].inner.clone();
                    bind(m.binding.as_ref().ok_or("an input without a binding")?, at, &mi, &mut c);
                    at += size(module, &mi);
                }
            }
            _ => return Err("an input without a binding".into()),
        }
    }
    c.input_range = (start, c.template.len());
    let info_ep = info.get_entry_point(entry);
    c.function(&ep.function, info_ep, args, None, true);
    Ok(Program {
        ops: c.ops,
        template: c.template,
        inputs: c.inputs,
        outputs: c.outputs,
        uniforms: c.uniforms,
        derivatives: c.derivatives,
        input_base: c.input_range.0,
        input_len: c.input_range.1 - c.input_range.0,
    })
}

/// Each scalar of a uniform value: (register, byte offset in the buffer), following naga's layout.
fn uniform_layout(module: &Module, ty: Handle<naga::Type>, offset: usize, reg: usize, out: &mut Vec<(usize, usize)>) -> usize {
    match &module.types[ty].inner {
        TypeInner::Scalar(_) => {
            out.push((reg, offset));
            1
        }
        TypeInner::Vector { size, .. } => {
            for i in 0..*size as usize {
                out.push((reg + i, offset + 4 * i));
            }
            *size as usize
        }
        TypeInner::Matrix { columns, rows, .. } => {
            let stride = if *rows as usize == 2 { 8 } else { 16 };
            for j in 0..*columns as usize {
                for i in 0..*rows as usize {
                    out.push((reg + j * *rows as usize + i, offset + j * stride + 4 * i));
                }
            }
            *columns as usize * *rows as usize
        }
        TypeInner::Struct { members, .. } => {
            let mut r = reg;
            for m in members {
                r += uniform_layout(module, m.ty, offset + m.offset as usize, r, out);
            }
            r - reg
        }
        TypeInner::Array { base, size: naga::ArraySize::Constant(n), stride } => {
            let mut r = reg;
            for i in 0..n.get() as usize {
                r += uniform_layout(module, *base, offset + i * *stride as usize, r, out);
            }
            r - reg
        }
        other => panic!("can't read {other:?} from a uniform buffer"),
    }
}

/// A program's registers, reused across invocations.
pub struct Registers(pub Vec<u32>);

impl Program {
    /// Fresh registers, with the uniforms read from the uniform buffer's bytes.
    pub fn registers(&self, uniform_bytes: &[u8]) -> Registers {
        let mut r = self.template.clone();
        for &(reg, offset) in &self.uniforms {
            r[reg] = uniform_bytes.get(offset..offset + 4).map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()));
        }
        Registers(r)
    }

    /// Run with the inputs already in the registers. False if a fragment was discarded. `gradients` are the
    /// inputs' derivatives (x then y, each `input_len` long), for a fragment shader that takes them.
    pub fn run(&self, regs: &mut Registers, texture: Option<&Image>, gradients: &[f32]) -> bool {
        let r = &mut regs.0;
        let f = |r: &[u32], i: usize| f32::from_bits(r[i]);
        let mut pc = 0;
        while pc < self.ops.len() {
            match &self.ops[pc] {
                Op::Copy { dst, src, n } => r.copy_within(*src..*src + *n, *dst),
                Op::Bin { op, k, dst, a, b, n, sa, sb } => {
                    for i in 0..*n {
                        let (x, y) = (r[if *sa { *a } else { a + i }], r[if *sb { *b } else { b + i }]);
                        r[dst + i] = binary(*op, *k, x, y);
                    }
                }
                Op::Un { op, k, dst, a, n } => {
                    for i in 0..*n {
                        let x = r[a + i];
                        r[dst + i] = match (op, k) {
                            (U::Negate, Kind::F) => (-f32::from_bits(x)).to_bits(),
                            (U::Negate, _) => (x as i32).wrapping_neg() as u32,
                            (U::LogicalNot, _) => (x == 0) as u32,
                            (U::BitwiseNot, _) => !x,
                        };
                    }
                }
                Op::Math { f: fun, k, dst, a, b, c, n, sb, sc } => math(r, *fun, *k, *dst, *a, *b, *c, *n, *sb, *sc),
                Op::MatVec { dst, m, v, cols, rows } => {
                    let mut out = [0f32; 4];
                    for i in 0..*rows {
                        out[i] = (0..*cols).map(|j| f(r, m + j * rows + i) * f(r, v + j)).sum();
                    }
                    for i in 0..*rows {
                        r[dst + i] = out[i].to_bits();
                    }
                }
                Op::VecMat { dst, v, m, cols, rows } => {
                    let mut out = [0f32; 4];
                    for j in 0..*cols {
                        out[j] = (0..*rows).map(|i| f(r, m + j * rows + i) * f(r, v + i)).sum();
                    }
                    for j in 0..*cols {
                        r[dst + j] = out[j].to_bits();
                    }
                }
                Op::MatMat { dst, a, b, rows, inner, cols } => {
                    let mut out = [0f32; 16];
                    for j in 0..*cols {
                        for i in 0..*rows {
                            out[j * rows + i] = (0..*inner).map(|k| f(r, a + k * rows + i) * f(r, b + j * inner + k)).sum();
                        }
                    }
                    for i in 0..cols * rows {
                        r[dst + i] = out[i].to_bits();
                    }
                }
                Op::Select { dst, c, a, b, n, sc } => {
                    for i in 0..*n {
                        let pick = r[if *sc { *c } else { c + i }] != 0;
                        r[dst + i] = if pick { r[a + i] } else { r[b + i] };
                    }
                }
                Op::Cast { dst, src, n, from, to, convert } => {
                    for i in 0..*n {
                        r[dst + i] = cast(r[src + i], *from, *to, *convert);
                    }
                }
                Op::Swizzle { dst, src, pattern, n } => {
                    let v = [r[*src], r[src + 1.min(3)], r.get(src + 2).copied().unwrap_or(0), r.get(src + 3).copied().unwrap_or(0)];
                    for i in 0..*n {
                        r[dst + i] = v[pattern[i] as usize];
                    }
                }
                Op::Splat { dst, src, n } => {
                    let v = r[*src];
                    r[*dst..*dst + *n].fill(v);
                }
                Op::Sample { dst, coord } => {
                    let t = texture.map_or([0.0; 4], |t| t.sample(f(r, *coord), f(r, coord + 1)));
                    for i in 0..4 {
                        r[dst + i] = t[i].to_bits();
                    }
                }
                Op::Derivative { dst, input, n, y } => {
                    let at = input - self.input_base + if *y { self.input_len } else { 0 };
                    for i in 0..*n {
                        r[dst + i] = gradients.get(at + i).copied().unwrap_or(0.0).to_bits();
                    }
                }
                Op::Addr { dst, base, index, stride, add } => r[*dst] = (*base + r[*index] as usize * *stride + *add) as u32,
                Op::LoadDyn { dst, addr, n } => {
                    let a = r[*addr] as usize;
                    r.copy_within(a..a + *n, *dst);
                }
                Op::StoreDyn { addr, src, n } => {
                    let a = r[*addr] as usize;
                    r.copy_within(*src..*src + *n, a);
                }
                Op::JumpIfZero { cond, to } => {
                    if r[*cond] == 0 {
                        pc = *to;
                        continue;
                    }
                }
                Op::JumpIfNonZero { cond, to } => {
                    if r[*cond] != 0 {
                        pc = *to;
                        continue;
                    }
                }
                Op::Jump { to } => {
                    pc = *to;
                    continue;
                }
                Op::Kill => return false,
            }
            pc += 1;
        }
        true
    }
}

fn binary(op: B, k: Kind, x: u32, y: u32) -> u32 {
    let (a, b) = (f32::from_bits(x), f32::from_bits(y));
    match (op, k) {
        (B::Add, Kind::F) => (a + b).to_bits(),
        (B::Subtract, Kind::F) => (a - b).to_bits(),
        (B::Multiply, Kind::F) => (a * b).to_bits(),
        (B::Divide, Kind::F) => (a / b).to_bits(),
        (B::Modulo, Kind::F) => (a - b * (a / b).trunc()).to_bits(),
        (B::Add, _) => x.wrapping_add(y),
        (B::Subtract, _) => x.wrapping_sub(y),
        (B::Multiply, _) => x.wrapping_mul(y),
        (B::Divide, Kind::I) => if y == 0 { 0 } else { (x as i32).wrapping_div(y as i32) as u32 },
        (B::Divide, _) => if y == 0 { 0 } else { x / y },
        (B::Modulo, Kind::I) => if y == 0 { 0 } else { (x as i32).wrapping_rem(y as i32) as u32 },
        (B::Modulo, _) => if y == 0 { 0 } else { x % y },
        (B::Equal, Kind::F) => (a == b) as u32,
        (B::NotEqual, Kind::F) => (a != b) as u32,
        (B::Less, Kind::F) => (a < b) as u32,
        (B::LessEqual, Kind::F) => (a <= b) as u32,
        (B::Greater, Kind::F) => (a > b) as u32,
        (B::GreaterEqual, Kind::F) => (a >= b) as u32,
        (B::Equal, _) => (x == y) as u32,
        (B::NotEqual, _) => (x != y) as u32,
        (B::Less, Kind::I) => ((x as i32) < (y as i32)) as u32,
        (B::LessEqual, Kind::I) => ((x as i32) <= (y as i32)) as u32,
        (B::Greater, Kind::I) => ((x as i32) > (y as i32)) as u32,
        (B::GreaterEqual, Kind::I) => ((x as i32) >= (y as i32)) as u32,
        (B::Less, _) => (x < y) as u32,
        (B::LessEqual, _) => (x <= y) as u32,
        (B::Greater, _) => (x > y) as u32,
        (B::GreaterEqual, _) => (x >= y) as u32,
        (B::And, _) => x & y,
        (B::InclusiveOr, _) => x | y,
        (B::ExclusiveOr, _) => x ^ y,
        (B::LogicalAnd, _) => (x != 0 && y != 0) as u32,
        (B::LogicalOr, _) => (x != 0 || y != 0) as u32,
        (B::ShiftLeft, _) => x.wrapping_shl(y),
        (B::ShiftRight, Kind::I) => ((x as i32).wrapping_shr(y)) as u32,
        (B::ShiftRight, _) => x.wrapping_shr(y),
    }
}

#[allow(clippy::too_many_arguments)]
fn math(r: &mut [u32], fun: M, k: Kind, dst: usize, a: usize, b: usize, c: usize, n: usize, sb: bool, sc: bool) {
    let f = |r: &[u32], i: usize| f32::from_bits(r[i]);
    let bi = |i: usize| if sb { b } else { b + i };
    let ci = |i: usize| if sc { c } else { c + i };
    let each = |r: &mut [u32], g: &dyn Fn(f32) -> f32| {
        for i in 0..n {
            r[dst + i] = g(f32::from_bits(r[a + i])).to_bits();
        }
    };
    let each2 = |r: &mut [u32], g: &dyn Fn(f32, f32) -> f32| {
        for i in 0..n {
            r[dst + i] = g(f32::from_bits(r[a + i]), f32::from_bits(r[bi(i)])).to_bits();
        }
    };
    match fun {
        M::Abs if k == Kind::F => each(r, &f32::abs),
        M::Abs => (0..n).for_each(|i| r[dst + i] = (r[a + i] as i32).unsigned_abs()),
        M::Min if k == Kind::F => each2(r, &f32::min),
        M::Max if k == Kind::F => each2(r, &f32::max),
        M::Min => (0..n).for_each(|i| r[dst + i] = if k == Kind::I { (r[a + i] as i32).min(r[bi(i)] as i32) as u32 } else { r[a + i].min(r[bi(i)]) }),
        M::Max if sc => {
            // `any`, compiled as a max over the vector's components
            r[dst] = (0..n).map(|i| r[a + i]).max().unwrap_or(0);
        }
        M::Max => (0..n).for_each(|i| r[dst + i] = if k == Kind::I { (r[a + i] as i32).max(r[bi(i)] as i32) as u32 } else { r[a + i].max(r[bi(i)]) }),
        M::Clamp => {
            for i in 0..n {
                r[dst + i] = f(r, a + i).max(f(r, bi(i))).min(f(r, ci(i))).to_bits();
            }
        }
        M::Saturate => each(r, &|v| v.clamp(0.0, 1.0)),
        M::Sin => each(r, &f32::sin),
        M::Cos => each(r, &f32::cos),
        M::Tan => each(r, &f32::tan),
        M::Asin => each(r, &f32::asin),
        M::Acos => each(r, &f32::acos),
        M::Atan => each(r, &f32::atan),
        M::Atan2 => each2(r, &f32::atan2),
        M::Sqrt => each(r, &f32::sqrt),
        M::InverseSqrt => each(r, &|v| 1.0 / v.sqrt()),
        M::Pow => each2(r, &f32::powf),
        M::Exp => each(r, &f32::exp),
        M::Exp2 => each(r, &f32::exp2),
        M::Log => each(r, &f32::ln),
        M::Log2 => each(r, &f32::log2),
        M::Floor => each(r, &f32::floor),
        M::Ceil => each(r, &f32::ceil),
        M::Round => each(r, &|v| v.round_ties_even()),
        M::Fract => each(r, &|v| v - v.floor()),
        M::Trunc => each(r, &f32::trunc),
        M::Sign => each(r, &|v| if v > 0.0 { 1.0 } else if v < 0.0 { -1.0 } else { 0.0 }),
        M::Step => {
            for i in 0..n {
                r[dst + i] = ((f(r, bi(i)) >= f(r, a + i)) as u32 as f32).to_bits();
            }
        }
        M::SmoothStep => {
            for i in 0..n {
                let t = ((f(r, ci(i)) - f(r, a + i)) / (f(r, bi(i)) - f(r, a + i))).clamp(0.0, 1.0);
                r[dst + i] = (t * t * (3.0 - 2.0 * t)).to_bits();
            }
        }
        M::Mix => {
            for i in 0..n {
                let t = f(r, ci(i));
                r[dst + i] = (f(r, a + i) * (1.0 - t) + f(r, bi(i)) * t).to_bits();
            }
        }
        M::Fma => {
            for i in 0..n {
                r[dst + i] = (f(r, a + i) * f(r, bi(i)) + f(r, ci(i))).to_bits();
            }
        }
        M::Dot => r[dst] = (0..n).map(|i| f(r, a + i) * f(r, b + i)).sum::<f32>().to_bits(),
        M::Cross => {
            let (x, y) = ([f(r, a), f(r, a + 1), f(r, a + 2)], [f(r, b), f(r, b + 1), f(r, b + 2)]);
            let out = [x[1] * y[2] - x[2] * y[1], x[2] * y[0] - x[0] * y[2], x[0] * y[1] - x[1] * y[0]];
            for i in 0..3 {
                r[dst + i] = out[i].to_bits();
            }
        }
        M::Length => r[dst] = (0..n).map(|i| f(r, a + i).powi(2)).sum::<f32>().sqrt().to_bits(),
        M::Distance => r[dst] = (0..n).map(|i| (f(r, a + i) - f(r, b + i)).powi(2)).sum::<f32>().sqrt().to_bits(),
        M::Normalize => {
            let len = (0..n).map(|i| f(r, a + i).powi(2)).sum::<f32>().sqrt();
            for i in 0..n {
                r[dst + i] = (f(r, a + i) / len).to_bits();
            }
        }
        M::Reflect => {
            let d = 2.0 * (0..n).map(|i| f(r, a + i) * f(r, b + i)).sum::<f32>();
            for i in 0..n {
                r[dst + i] = (f(r, a + i) - d * f(r, b + i)).to_bits();
            }
        }
        other => panic!("the still can't compute {other:?}"),
    }
}

fn cast(x: u32, from: Kind, to: Kind, convert: bool) -> u32 {
    if !convert {
        return x; // a bitcast: the same bits
    }
    match (from, to) {
        (Kind::F, Kind::F) | (Kind::I, Kind::I) | (Kind::U, Kind::U) => x,
        (Kind::F, Kind::U) => f32::from_bits(x).max(0.0) as u32,
        (Kind::F, Kind::I) => f32::from_bits(x) as i32 as u32,
        (Kind::I, Kind::F) => (x as i32 as f32).to_bits(),
        (Kind::U, Kind::F) => (x as f32).to_bits(),
        (Kind::B, Kind::F) => (x as f32).to_bits(),
        (Kind::I, Kind::U) | (Kind::U, Kind::I) => x,
        (_, Kind::B) => (if from == Kind::F { f32::from_bits(x) != 0.0 } else { x != 0 }) as u32,
        (Kind::B, _) => x,
    }
}
