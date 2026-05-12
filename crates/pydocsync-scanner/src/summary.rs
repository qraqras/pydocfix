use crate::ByteRange;

/// A lightweight summary of Python source relevant to docstring tooling.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileSummary {
    /// Function and async function items in depth-first source order.
    pub items: Vec<FunctionItem>,
}

/// A summarized Python function definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionItem {
    /// Function name.
    pub name: String,
    /// Byte range of the function name.
    pub name_range: ByteRange,
    /// Byte range from `def` or `async` through the header colon.
    pub header_range: ByteRange,
    /// Byte range from the opening `(` through the matching `)`.
    pub params_range: ByteRange,
    /// Byte range of the return annotation, including the leading `->`.
    pub return_annotation_range: Option<ByteRange>,
    /// Function docstring token range, including any string prefix and quotes.
    pub docstring_range: Option<ByteRange>,
    /// Whether this function was introduced with `async def`.
    pub is_async: bool,
    /// Whether this function is lexically nested inside a class.
    pub is_method: bool,
    /// Function signature parameters in source order.
    pub parameters: Vec<ParameterRecord>,
    /// Whether the function contains a non-bare `return` in its own scope.
    pub has_return_value: bool,
    /// Whether the function contains `yield` or `yield from` in its own scope.
    pub has_yield: bool,
}

/// A function signature parameter found in a function header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParameterRecord {
    /// Display name, including `*` or `**` for varargs and kwargs.
    pub name: String,
    /// Bare name without `*` or `**` prefixes.
    pub bare_name: String,
    /// Byte range of the name token, excluding `*` and `**` prefixes.
    pub name_range: ByteRange,
    /// Byte range of the type annotation, excluding the leading colon.
    pub annotation_range: Option<ByteRange>,
    /// Byte range of the default value, excluding the leading equals sign.
    pub default_range: Option<ByteRange>,
    /// Whether this parameter is `*args`.
    pub is_vararg: bool,
    /// Whether this parameter is `**kwargs`.
    pub is_kwarg: bool,
    /// Whether this parameter is the first `self` or `cls` method parameter.
    pub is_implicit_receiver: bool,
}
