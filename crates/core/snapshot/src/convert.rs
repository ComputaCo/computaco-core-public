//! Reading a body another build wrote: its schema says where each field is, and this
//! build's says where it goes, so the body is rewritten field by field (matched by
//! name) into the bytes this build would write, and read from those.
use super::schema::{short, Field, Key, Kind, Prim, Schema, Ty};
use super::*;
use std::collections::HashMap;

/// Where a reader field's value comes from.
#[derive(Clone, Copy)]
enum Fill {
    From,
    Default(fn(&mut Writer)),
}
/// How one struct (or variant) of the snapshot becomes this build's.
struct Plan {
    /// The variant number this build writes, and where the two variants are in their
    /// enums (structs: unused).
    number: u64,
    variants: (usize, usize),
    /// For each of the snapshot's fields, the field of this build's it fills.
    to: Box<[Option<usize>]>,
    from: Box<[Fill]>,
    /// The snapshot's fields fill this build's in order, so they are streamed.
    ordered: bool,
    label: String,
}

pub(crate) struct Convert<'s, 'a> {
    ws: &'s Schema,
    rs: &'s Schema,
    data: &'a [u8],
    r: Reader<'a>,
    out: Writer,
    plans: HashMap<(Ty, Ty, u64), std::rc::Rc<Plan>>,
    /// Values left to visit: a schema can make a value of no bytes (a unit in a
    /// sequence), so bytes alone do not bound the work.
    budget: u64,
}

fn what(s: &Schema, t: Ty) -> String {
    match s.kind(t) {
        Kind::Key(Key::Prim(p)) => match p {
            Prim::Byte => "a byte".into(),
            Prim::Uleb => "an unsigned integer".into(),
            Prim::Sleb => "a signed integer".into(),
            Prim::F32 => "an f32".into(),
            Prim::F64 => "an f64".into(),
            Prim::U128 => "a u128".into(),
            Prim::I128 => "an i128".into(),
            Prim::Bool => "a bool".into(),
            Prim::Char => "a char".into(),
            Prim::Unit => "a unit".into(),
            Prim::Str => "text".into(),
            Prim::Json => "JSON".into(),
        },
        Kind::Key(Key::Option(_)) => "an option".into(),
        Kind::Key(Key::Result(..)) => "a result".into(),
        Kind::Key(Key::Seq(_)) => "a sequence".into(),
        Kind::Key(Key::Set(_)) => "a set".into(),
        Kind::Key(Key::Map(..)) => "a map".into(),
        Kind::Key(Key::Tuple(items)) => format!("a tuple of {}", items.len()),
        Kind::Key(Key::Array(n, _)) => format!("an array of {n}"),
        Kind::Key(Key::Nested(_)) => "a nested value".into(),
        Kind::Struct { label: "", .. } => "a struct".into(),
        Kind::Struct { label, .. } => short(label),
        Kind::Enum { label: "", .. } => "an enum".into(),
        Kind::Enum { label, .. } => short(label),
    }
}

impl<'s, 'a> Convert<'s, 'a> {
    pub(crate) fn new(ws: &'s Schema, rs: &'s Schema, data: &'a [u8], start: usize) -> Self {
        Self {
            ws,
            rs,
            data,
            r: Reader {
                data,
                pos: start,
                depth: 0,
                lenient: false,
            },
            out: Writer::with_capacity(data.len()),
            plans: HashMap::new(),
            budget: (data.len() as u64).saturating_mul(64) + (1 << 20),
        }
    }
    /// The body, rewritten from the snapshot's `wt` into this build's `rt`.
    pub(crate) fn run(mut self, wt: Ty, rt: Ty) -> Result<Vec<u8>, Error> {
        self.value(wt, rt)?;
        self.r.finish()?;
        Ok(self.out.buf)
    }
    /// Walk the body as the snapshot's `wt`, keeping nothing: whether it is one.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub(crate) fn check(mut self, wt: Ty) -> Result<(), Error> {
        self.skip(wt)?;
        self.r.finish()
    }

    #[inline]
    fn tick(&mut self) -> Result<(), Error> {
        match self.budget.checked_sub(1) {
            Some(b) => {
                self.budget = b;
                Ok(())
            }
            None => Err(self
                .r
                .error("the snapshot holds more values than its bytes can")),
        }
    }

    /// Run `f` over the `n` bytes a nested value holds.
    fn within(&mut self, f: impl FnOnce(&mut Self) -> Result<(), Error>) -> Result<(), Error> {
        let n = self.r.len_prefix()?;
        let (start, end, depth) = (self.r.pos, self.r.pos + n, self.r.depth);
        let inner = Reader {
            data: &self.data[..end],
            pos: start,
            depth,
            lenient: false,
        };
        let outer = std::mem::replace(&mut self.r, inner);
        let out = f(self).and_then(|()| self.r.finish());
        self.r = Reader { pos: end, ..outer };
        out
    }

    fn prim(&mut self, p: Prim) -> Result<(), Error> {
        let r = &mut self.r;
        match p {
            Prim::Byte | Prim::Bool => {
                r.byte()?;
            }
            Prim::Uleb | Prim::Sleb | Prim::Char => {
                r.leb()?;
            }
            Prim::F32 => {
                r.raw(4)?;
            }
            Prim::F64 => {
                r.raw(8)?;
            }
            Prim::U128 | Prim::I128 => {
                r.raw(16)?;
            }
            Prim::Unit => {}
            Prim::Str => {
                r.bytes()?;
            }
            Prim::Json => self.json()?,
        }
        Ok(())
    }
    fn json(&mut self) -> Result<(), Error> {
        self.tick()?;
        let at = self.r.pos;
        match self.r.byte()? {
            0..=2 => {}
            3 | 4 => {
                self.r.leb()?;
            }
            5 => {
                self.r.raw(8)?;
            }
            6 | 9 => {
                self.r.bytes()?;
            }
            7 => {
                let n = self.r.len_prefix()?;
                self.nested(|c| (0..n).try_for_each(|_| c.json()))?;
            }
            8 => {
                let n = self.r.len_prefix()?;
                self.nested(|c| {
                    (0..n).try_for_each(|_| {
                        c.r.bytes()?;
                        c.json()
                    })
                })?;
            }
            tag => return Err(Error::at(at, format!("{tag} is not a JSON tag"))),
        }
        Ok(())
    }
    fn nested(&mut self, f: impl FnOnce(&mut Self) -> Result<(), Error>) -> Result<(), Error> {
        if self.r.depth >= MAX_DEPTH {
            return Err(self.r.error("nested too deeply"));
        }
        self.r.depth += 1;
        let out = f(self);
        self.r.depth -= 1;
        out
    }
    fn tag(&mut self, what: &str) -> Result<bool, Error> {
        match self.r.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            b => Err(Error::at(self.r.pos - 1, format!("{b} is not {what} tag"))),
        }
    }

    /// Walk one value of the snapshot's type `t`.
    fn skip(&mut self, t: Ty) -> Result<(), Error> {
        self.tick()?;
        let ws = self.ws;
        match ws.kind(t) {
            Kind::Key(Key::Prim(p)) => self.prim(*p),
            Kind::Key(Key::Option(a)) => {
                if self.tag("an option")? {
                    self.nested(|c| c.skip(*a))?;
                }
                Ok(())
            }
            Kind::Key(Key::Result(a, e)) => {
                let t = if self.tag("a result")? { e } else { a };
                self.nested(|c| c.skip(*t))
            }
            Kind::Key(Key::Seq(a) | Key::Set(a)) => {
                let n = self.r.len_prefix()?;
                if matches!(ws.kind(*a), Kind::Key(Key::Prim(Prim::Byte))) {
                    self.r.raw(n)?;
                    return Ok(());
                }
                self.nested(|c| (0..n).try_for_each(|_| c.skip(*a)))
            }
            Kind::Key(Key::Map(k, v)) => {
                let n = self.r.len_prefix()?;
                self.nested(|c| {
                    (0..n).try_for_each(|_| {
                        c.skip(*k)?;
                        c.skip(*v)
                    })
                })
            }
            Kind::Key(Key::Tuple(items)) => {
                self.nested(|c| items.iter().try_for_each(|t| c.skip(*t)))
            }
            Kind::Key(Key::Array(n, a)) => self.nested(|c| (0..*n).try_for_each(|_| c.skip(*a))),
            Kind::Key(Key::Nested(a)) => self.within(|c| c.skip(*a)),
            Kind::Struct { fields, .. } => {
                self.nested(|c| fields.iter().try_for_each(|f| c.skip(f.ty)))
            }
            Kind::Enum { variants, .. } => {
                let at = self.r.pos;
                let n = self.r.leb()?;
                let v = variants.iter().find(|v| v.number == n).ok_or_else(|| {
                    Error::at(at, format!("{n} is not a variant of {}", what(ws, t)))
                })?;
                self.nested(|c| v.fields.iter().try_for_each(|f| c.skip(f.ty)))
            }
        }
    }

    fn mismatch(&self, wt: Ty, rt: Ty) -> Error {
        self.r.error(format!(
            "the snapshot holds {} where this build reads {}",
            what(self.ws, wt),
            what(self.rs, rt)
        ))
    }

    /// Rewrite one value of the snapshot's `wt` as this build's `rt`.
    fn value(&mut self, wt: Ty, rt: Ty) -> Result<(), Error> {
        self.tick()?;
        let (ws, rs) = (self.ws, self.rs);
        match (ws.kind(wt), rs.kind(rt)) {
            (Kind::Key(Key::Prim(a)), Kind::Key(Key::Prim(b))) if a == b => {
                let start = self.r.pos;
                self.prim(*a)?;
                self.out.raw(&self.data[start..self.r.pos]);
                Ok(())
            }
            // A byte widened to a wider integer, or narrowed back while it fits.
            (Kind::Key(Key::Prim(Prim::Byte)), Kind::Key(Key::Prim(Prim::Uleb))) => {
                let b = self.r.byte()?;
                self.out.leb(b.into());
                Ok(())
            }
            (Kind::Key(Key::Prim(Prim::Uleb)), Kind::Key(Key::Prim(Prim::Byte))) => {
                let at = self.r.pos;
                let v = self.r.leb()?;
                let b = u8::try_from(v)
                    .map_err(|_| Error::at(at, format!("{v} is out of range for u8")))?;
                self.out.byte(b);
                Ok(())
            }
            (Kind::Key(Key::Option(a)), Kind::Key(Key::Option(b))) => {
                let some = self.tag("an option")?;
                self.out.byte(some.into());
                if some {
                    self.nested(|c| c.value(*a, *b))?;
                }
                Ok(())
            }
            (Kind::Key(Key::Result(a, e)), Kind::Key(Key::Result(b, f))) => {
                let err = self.tag("a result")?;
                self.out.byte(err.into());
                let (w, r) = if err { (e, f) } else { (a, b) };
                self.nested(|c| c.value(*w, *r))
            }
            (Kind::Key(Key::Seq(a) | Key::Set(a)), Kind::Key(Key::Seq(b) | Key::Set(b))) => {
                let n = self.r.len_prefix()?;
                self.out.len_prefix(n);
                let bytes =
                    |s: &Schema, t: Ty| matches!(s.kind(t), Kind::Key(Key::Prim(Prim::Byte)));
                if bytes(ws, *a) && bytes(rs, *b) {
                    let raw = self.r.raw(n)?;
                    self.out.raw(raw);
                    return Ok(());
                }
                self.nested(|c| (0..n).try_for_each(|_| c.value(*a, *b)))
            }
            (Kind::Key(Key::Map(k, v)), Kind::Key(Key::Map(k2, v2))) => {
                let n = self.r.len_prefix()?;
                self.out.len_prefix(n);
                self.nested(|c| {
                    (0..n).try_for_each(|_| {
                        c.value(*k, *k2)?;
                        c.value(*v, *v2)
                    })
                })
            }
            (Kind::Key(Key::Tuple(a)), Kind::Key(Key::Tuple(b))) if a.len() == b.len() => self
                .nested(|c| {
                    a.iter()
                        .zip(b.iter())
                        .try_for_each(|(w, r)| c.value(*w, *r))
                }),
            (Kind::Key(Key::Array(n, a)), Kind::Key(Key::Array(m, b))) if n == m => {
                self.nested(|c| (0..*n).try_for_each(|_| c.value(*a, *b)))
            }
            (Kind::Key(Key::Nested(a)), Kind::Key(Key::Nested(b))) => {
                let outer = std::mem::take(&mut self.out);
                let done = self.within(|c| c.value(*a, *b));
                let inner = std::mem::replace(&mut self.out, outer);
                done?;
                self.out.bytes(&inner.buf);
                Ok(())
            }
            (
                Kind::Struct { fields: wf, .. },
                Kind::Struct {
                    label, fields: rf, ..
                },
            ) => {
                let plan = self.plan((wt, rt, u64::MAX), wf, rf, (0, (0, 0)), label, "")?;
                self.nested(|c| c.fields(&plan, wf, rf))
            }
            (
                Kind::Enum { variants: wv, .. },
                Kind::Enum {
                    label,
                    variants: rv,
                },
            ) => {
                let at = self.r.pos;
                let n = self.r.leb()?;
                let plan = match self.plans.get(&(wt, rt, n)) {
                    Some(p) => p.clone(),
                    None => {
                        let i = wv.iter().position(|v| v.number == n).ok_or_else(|| {
                            Error::at(at, format!("{n} is not a variant of {}", what(ws, wt)))
                        })?;
                        let name = &ws.names[wv[i].name as usize];
                        let j = rv
                            .iter()
                            .position(|r| *rs.names[r.name as usize] == **name)
                            .ok_or_else(|| {
                                Error::at(
                                    at,
                                    format!(
                                        "the snapshot holds {}::{name}, which this build does not have",
                                        short(label)
                                    ),
                                )
                            })?;
                        let (wf, rf) = (&wv[i].fields, &rv[j].fields);
                        self.plan((wt, rt, n), wf, rf, (rv[j].number, (i, j)), label, name)
                            .map_err(|e| Error::at(at, e.message))?
                    }
                };
                self.out.leb(plan.number);
                let (wf, rf) = (&wv[plan.variants.0].fields, &rv[plan.variants.1].fields);
                self.nested(|c| c.fields(&plan, wf, rf))
            }
            _ => Err(self.mismatch(wt, rt)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn plan(
        &mut self,
        key: (Ty, Ty, u64),
        wf: &[Field],
        rf: &[Field],
        (number, variants): (u64, (usize, usize)),
        label: &str,
        variant: &str,
    ) -> Result<std::rc::Rc<Plan>, Error> {
        if let Some(p) = self.plans.get(&key) {
            return Ok(p.clone());
        }
        let (ws, rs) = (self.ws, self.rs);
        let label = if variant.is_empty() {
            short(label)
        } else {
            format!("{}::{variant}", short(label))
        };
        let mut to = vec![None; wf.len()];
        let mut from = Vec::with_capacity(rf.len());
        for (j, f) in rf.iter().enumerate() {
            let name = &rs.names[f.name as usize];
            match wf.iter().position(|w| ws.names[w.name as usize] == *name) {
                Some(i) => {
                    to[i] = Some(j);
                    from.push(Fill::From);
                }
                None => match f.default {
                    Some(d) => from.push(Fill::Default(d)),
                    None => {
                        return Err(self.r.error(format!(
                            "{label} has a field `{name}` the snapshot lacks, and no default to fill it with"
                        )))
                    }
                },
            }
        }
        let mapped: Vec<usize> = to.iter().flatten().copied().collect();
        let plan = std::rc::Rc::new(Plan {
            number,
            variants,
            ordered: mapped.windows(2).all(|w| w[0] < w[1]),
            to: to.into(),
            from: from.into(),
            label,
        });
        self.plans.insert(key, plan.clone());
        Ok(plan)
    }

    fn fill(&mut self, plan: &Plan, j: usize) -> Result<(), Error> {
        match plan.from[j] {
            Fill::Default(d) => {
                d(&mut self.out);
                Ok(())
            }
            Fill::From => Err(self
                .r
                .error(format!("{}: a field is out of place", plan.label))),
        }
    }

    fn field(&mut self, plan: &Plan, w: &Field, r: &Field) -> Result<(), Error> {
        self.value(w.ty, r.ty).map_err(|e| {
            Error::at(
                e.offset,
                format!(
                    "{}.{}: {}",
                    plan.label, self.rs.names[r.name as usize], e.message
                ),
            )
        })
    }

    fn fields(&mut self, plan: &Plan, wf: &[Field], rf: &[Field]) -> Result<(), Error> {
        if plan.ordered {
            let mut next = 0;
            for (i, w) in wf.iter().enumerate() {
                match plan.to[i] {
                    Some(j) => {
                        for k in next..j {
                            self.fill(plan, k)?;
                        }
                        self.field(plan, w, &rf[j])?;
                        next = j + 1;
                    }
                    None => self.skip(w.ty)?,
                }
            }
            for k in next..rf.len() {
                self.fill(plan, k)?;
            }
            return Ok(());
        }
        let mut slots: Vec<Option<Vec<u8>>> = vec![None; rf.len()];
        for (i, w) in wf.iter().enumerate() {
            match plan.to[i] {
                Some(j) => {
                    let outer = std::mem::take(&mut self.out);
                    let done = self.field(plan, w, &rf[j]);
                    slots[j] = Some(std::mem::replace(&mut self.out, outer).buf);
                    done?;
                }
                None => self.skip(w.ty)?,
            }
        }
        for (k, slot) in slots.iter().enumerate() {
            match slot {
                Some(bytes) => self.out.raw(bytes),
                None => self.fill(plan, k)?,
            }
        }
        Ok(())
    }
}
