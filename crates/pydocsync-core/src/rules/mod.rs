use docstring_cst::Source;
use docstring_cst::semantic::SemanticView;

use crate::DocstringHost;

pub(crate) mod parameters;
pub(crate) mod raises;
pub(crate) mod returns;
pub(crate) mod yields;

pub(crate) struct RuleContext<'a> {
    pub(crate) source: &'a Source,
    pub(crate) host: &'a DocstringHost,
    pub(crate) semantic: &'a SemanticView,
}
