#![feature(arbitrary_self_types)]
#![feature(arbitrary_self_types_pointers)]
#![allow(clippy::needless_return)] // tokio macro-generated code doesn't respect this

use std::fmt;

use turbo_rcstr::RcStr;
use turbo_tasks::{ResolvedVc, ValueToString, Vc};
use turbo_tasks_testing::{Registration, register, run_once};

static REGISTRATION: Registration = register!();

// --- Test types ---

/// A struct that implements Display and uses the derive without a format string.
/// The derive delegates to Display::to_string(self).
#[turbo_tasks::value(shared)]
#[derive(ValueToString)]
struct SimpleDisplay(u32);

impl fmt::Display for SimpleDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "simple:{}", self.0)
    }
}

/// A struct with a format string attribute that references named fields.
#[turbo_tasks::value(shared)]
#[derive(ValueToString)]
#[value_to_string("item {name} (count: {count})")]
struct NamedFields {
    name: RcStr,
    count: u32,
}

/// A struct with a format string that references positional fields.
#[turbo_tasks::value(shared)]
#[derive(ValueToString)]
#[value_to_string("wrapped({0})")]
struct TupleStruct(u32);

/// An enum with per-variant format strings.
#[turbo_tasks::value(shared)]
#[derive(ValueToString)]
enum Kind {
    #[value_to_string("module")]
    Module,
    #[value_to_string("asset({0})")]
    Asset(RcStr),
    #[value_to_string("entry {name}")]
    Entry { name: RcStr },
}

/// An enum that defaults variant names when no attribute is given.
#[turbo_tasks::value(shared)]
#[derive(ValueToString)]
enum DefaultNames {
    Alpha,
    Beta,
}

/// A struct using the direct expression form.
#[turbo_tasks::value(shared)]
#[derive(ValueToString)]
#[value_to_string(self.name)]
struct DirectExpr {
    name: RcStr,
    #[allow(dead_code)]
    other: u32,
}

/// A struct using the constant string form (no field references).
#[turbo_tasks::value(shared)]
#[derive(ValueToString)]
#[value_to_string("constant-value")]
struct ConstantString;

/// A struct using format string with expression arguments.
#[turbo_tasks::value(shared)]
#[derive(ValueToString)]
#[value_to_string("prefix({}) suffix({})", self.name, self.count)]
struct FormatExprs {
    name: RcStr,
    count: u32,
}

/// A struct that delegates to a Vc field via expression.
#[turbo_tasks::value(shared)]
#[derive(ValueToString)]
#[value_to_string("inner: {}", self.inner)]
struct VcExprDelegate {
    inner: ResolvedVc<NamedFields>,
}

/// An enum with mixed forms.
#[turbo_tasks::value(shared)]
#[derive(ValueToString)]
enum MixedEnum {
    #[value_to_string("literal")]
    Literal,
    #[value_to_string(_0)]
    Delegate(ResolvedVc<ConstantString>),
    #[value_to_string("wrapped({})", name)]
    ExprNamed { name: RcStr },
}

// --- Tests ---

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_simple_display_delegation() {
    run_once(&REGISTRATION, || async {
        let v: Vc<SimpleDisplay> = SimpleDisplay(42).cell();
        let s = v.to_string().await?;
        assert_eq!(&*s, "simple:42");
        anyhow::Ok(())
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_named_fields_format() {
    run_once(&REGISTRATION, || async {
        let v: Vc<NamedFields> = NamedFields {
            name: "foo".into(),
            count: 7,
        }
        .cell();
        let s = v.to_string().await?;
        assert_eq!(&*s, "item foo (count: 7)");
        anyhow::Ok(())
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tuple_struct_format() {
    run_once(&REGISTRATION, || async {
        let v: Vc<TupleStruct> = TupleStruct(99).cell();
        let s = v.to_string().await?;
        assert_eq!(&*s, "wrapped(99)");
        anyhow::Ok(())
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_enum_unit_variant() {
    run_once(&REGISTRATION, || async {
        let v: Vc<Kind> = Kind::Module.cell();
        let s = v.to_string().await?;
        assert_eq!(&*s, "module");
        anyhow::Ok(())
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_enum_tuple_variant() {
    run_once(&REGISTRATION, || async {
        let v: Vc<Kind> = Kind::Asset("main.js".into()).cell();
        let s = v.to_string().await?;
        assert_eq!(&*s, "asset(main.js)");
        anyhow::Ok(())
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_enum_named_variant() {
    run_once(&REGISTRATION, || async {
        let v: Vc<Kind> = (Kind::Entry {
            name: "index".into(),
        })
        .cell();
        let s = v.to_string().await?;
        assert_eq!(&*s, "entry index");
        anyhow::Ok(())
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_enum_default_variant_names() {
    run_once(&REGISTRATION, || async {
        let v1: Vc<DefaultNames> = DefaultNames::Alpha.cell();
        let s1 = v1.to_string().await?;
        assert_eq!(&*s1, "Alpha");

        let v2: Vc<DefaultNames> = DefaultNames::Beta.cell();
        let s2 = v2.to_string().await?;
        assert_eq!(&*s2, "Beta");

        anyhow::Ok(())
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_direct_expression() {
    run_once(&REGISTRATION, || async {
        let v: Vc<DirectExpr> = DirectExpr {
            name: "hello".into(),
            other: 42,
        }
        .cell();
        let s = v.to_string().await?;
        assert_eq!(&*s, "hello");
        anyhow::Ok(())
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_constant_string() {
    run_once(&REGISTRATION, || async {
        let v: Vc<ConstantString> = ConstantString.cell();
        let s = v.to_string().await?;
        assert_eq!(&*s, "constant-value");
        anyhow::Ok(())
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_format_with_expressions() {
    run_once(&REGISTRATION, || async {
        let v: Vc<FormatExprs> = FormatExprs {
            name: "test".into(),
            count: 5,
        }
        .cell();
        let s = v.to_string().await?;
        assert_eq!(&*s, "prefix(test) suffix(5)");
        anyhow::Ok(())
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_vc_expression_delegate() {
    run_once(&REGISTRATION, || async {
        let inner = NamedFields {
            name: "bar".into(),
            count: 3,
        }
        .resolved_cell();
        let v: Vc<VcExprDelegate> = VcExprDelegate { inner }.cell();
        let s = v.to_string().await?;
        assert_eq!(&*s, "inner: item bar (count: 3)");
        anyhow::Ok(())
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_mixed_enum() {
    run_once(&REGISTRATION, || async {
        let v1: Vc<MixedEnum> = MixedEnum::Literal.cell();
        let s1 = v1.to_string().await?;
        assert_eq!(&*s1, "literal");

        let inner = ConstantString.resolved_cell();
        let v2: Vc<MixedEnum> = MixedEnum::Delegate(inner).cell();
        let s2 = v2.to_string().await?;
        assert_eq!(&*s2, "constant-value");

        let v3: Vc<MixedEnum> = (MixedEnum::ExprNamed {
            name: "world".into(),
        })
        .cell();
        let s3 = v3.to_string().await?;
        assert_eq!(&*s3, "wrapped(world)");

        anyhow::Ok(())
    })
    .await
    .unwrap()
}
