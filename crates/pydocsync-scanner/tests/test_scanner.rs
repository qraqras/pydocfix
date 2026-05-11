use pydocsync_scanner::{ByteRange, Item, summarize_python};

fn slice(source: &str, range: ByteRange) -> &str {
    &source[range.start()..range.end()]
}

#[test]
fn summarizes_module_class_and_function_docstrings() {
    let source = r#"# leading comment
"""Module docs."""

class Example:
    """Class docs."""

    def method(self, value: int) -> str:
        """Method docs."""
        return str(value)
"#;

    let summary = summarize_python(source);
    assert_eq!(
        summary.module_docstring.map(|range| slice(source, range)),
        Some("\"\"\"Module docs.\"\"\"")
    );
    assert_eq!(summary.items.len(), 2);

    let class = match &summary.items[0] {
        Item::Class(item) => item,
        item => panic!("expected class, got {item:?}"),
    };
    assert_eq!(class.name, "Example");
    assert_eq!(slice(source, class.docstring_range.unwrap()), "\"\"\"Class docs.\"\"\"");
    assert_eq!(class.parent_index, None);

    let function = match &summary.items[1] {
        Item::Function(item) => item,
        item => panic!("expected function, got {item:?}"),
    };
    assert_eq!(function.name, "method");
    assert!(function.is_method);
    assert_eq!(function.parent_index, Some(0));
    assert_eq!(slice(source, function.params_range), "(self, value: int)");
    assert_eq!(
        function.return_annotation_range.map(|range| slice(source, range)),
        Some("-> str")
    );
    assert_eq!(
        slice(source, function.docstring_range.unwrap()),
        "\"\"\"Method docs.\"\"\""
    );
    assert!(function.has_return_value);
    assert!(!function.has_yield);
    assert_eq!(function.parameters.len(), 2);
    assert_eq!(function.parameters[0].name, "self");
    assert!(function.parameters[0].is_implicit_receiver);
    assert_eq!(function.parameters[1].name, "value");
    assert_eq!(slice(source, function.parameters[1].annotation_range.unwrap()), "int");
}

#[test]
fn records_signature_parameter_facts() {
    let source = r#"def example(
    x: int,
    y: list[str] = None,
    *args: float,
    flag=False,
    **kwargs: str,
):
    pass
"#;

    let summary = summarize_python(source);
    let function = match &summary.items[0] {
        Item::Function(item) => item,
        item => panic!("expected function, got {item:?}"),
    };

    let names: Vec<&str> = function.parameters.iter().map(|param| param.name.as_str()).collect();
    assert_eq!(names, vec!["x", "y", "*args", "flag", "**kwargs"]);
    assert_eq!(slice(source, function.parameters[0].annotation_range.unwrap()), "int");
    assert_eq!(
        slice(source, function.parameters[1].annotation_range.unwrap()),
        "list[str]"
    );
    assert_eq!(slice(source, function.parameters[1].default_range.unwrap()), "None");
    assert!(function.parameters[2].is_vararg);
    assert_eq!(slice(source, function.parameters[2].annotation_range.unwrap()), "float");
    assert_eq!(slice(source, function.parameters[3].default_range.unwrap()), "False");
    assert!(function.parameters[4].is_kwarg);
    assert_eq!(slice(source, function.parameters[4].annotation_range.unwrap()), "str");
}

#[test]
fn records_decorators_async_yield_and_raises() {
    let source = r#"class Service:
    @classmethod
    @decorator(arg="x")
    async def build(cls, value):
        """Build docs."""
        if value < 0:
            raise ValueError("bad")
        yield value
"#;

    let summary = summarize_python(source);
    assert_eq!(summary.items.len(), 2);
    let function = match &summary.items[1] {
        Item::Function(item) => item,
        item => panic!("expected function, got {item:?}"),
    };

    assert_eq!(function.name, "build");
    assert!(function.is_async);
    assert_eq!(function.decorators.len(), 2);
    assert_eq!(slice(source, function.decorators[0]), "@classmethod");
    assert_eq!(slice(source, function.decorators[1]), "@decorator(arg=\"x\")");
    assert_eq!(function.raises.len(), 1);
    assert_eq!(slice(source, function.raises[0].name_range), "ValueError");
    assert!(!function.raises[0].from_bare_except);
    assert!(function.has_yield);
}

#[test]
fn skips_nested_function_facts_from_parent() {
    let source = r#"def outer():
    """Outer docs."""
    def inner():
        raise RuntimeError("inner")
    return 1
"#;

    let summary = summarize_python(source);
    assert_eq!(summary.items.len(), 2);
    let outer = match &summary.items[0] {
        Item::Function(item) => item,
        item => panic!("expected function, got {item:?}"),
    };
    let inner = match &summary.items[1] {
        Item::Function(item) => item,
        item => panic!("expected function, got {item:?}"),
    };

    assert_eq!(outer.name, "outer");
    assert!(outer.has_return_value);
    assert!(outer.raises.is_empty());
    assert_eq!(inner.name, "inner");
    assert_eq!(inner.raises.len(), 1);
    assert_eq!(slice(source, inner.raises[0].name_range), "RuntimeError");
}

#[test]
fn bare_return_and_return_none_do_not_count_as_return_values() {
    let source = r#"def first():
    return

def second():
    return None

def third():
    return None  # explicit no value

def fourth():
    return NoneValue
"#;

    let summary = summarize_python(source);
    let values: Vec<bool> = summary
        .items
        .iter()
        .map(|item| match item {
            Item::Function(function) => function.has_return_value,
            item => panic!("expected function, got {item:?}"),
        })
        .collect();

    assert_eq!(values, vec![false, false, false, true]);
}

#[test]
fn records_bare_reraise_from_except_type() {
    let source = r#"def parse(value):
    try:
        int(value)
    except ValueError as exc:
        raise
"#;

    let summary = summarize_python(source);
    let function = match &summary.items[0] {
        Item::Function(item) => item,
        item => panic!("expected function, got {item:?}"),
    };

    assert_eq!(function.raises.len(), 1);
    assert_eq!(slice(source, function.raises[0].name_range), "ValueError");
    assert!(function.raises[0].from_bare_except);
}

#[test]
fn tolerates_unterminated_strings() {
    let source = "def broken():\n    \"\"\"unterminated\n    return 1\n";
    let summary = summarize_python(source);
    let function = match &summary.items[0] {
        Item::Function(item) => item,
        item => panic!("expected function, got {item:?}"),
    };

    assert_eq!(function.name, "broken");
    assert_eq!(function.docstring_range, None);
}
