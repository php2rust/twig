//! The node class hierarchy of `twig/twig` `src/Node`: each class with its
//! parent, whether it is abstract, and the interfaces it implements
//! (inherited ones included), as `instanceof` sees them. Derived from the
//! class declarations; the classes' constructors and behaviour are in the
//! sibling modules.
//!
//! Upstream: none

// A generated table: one match arm per class.
#![allow(clippy::too_many_lines, clippy::match_same_arms)]

/// A node class of `Twig\Node`: every class, abstract ones included.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeClass {
    AutoEscapeNode,
    BlockNode,
    BlockReferenceNode,
    BodyNode,
    CaptureNode,
    CheckSecurityCallNode,
    CheckSecurityNode,
    CheckToStringNode,
    ConfigNode,
    DeprecatedNode,
    DoNode,
    EmbedNode,
    EmptyNode,
    AbstractExpression,
    ArrayExpression,
    ArrowFunctionExpression,
    AssignNameExpression,
    AbstractBinary,
    AddBinary,
    AndBinary,
    BitwiseAndBinary,
    BitwiseOrBinary,
    BitwiseXorBinary,
    ConcatBinary,
    DivBinary,
    ElvisBinary,
    EndsWithBinary,
    EqualBinary,
    FloorDivBinary,
    GreaterBinary,
    GreaterEqualBinary,
    HasEveryBinary,
    HasSomeBinary,
    InBinary,
    LessBinary,
    LessEqualBinary,
    MatchesBinary,
    ModBinary,
    MulBinary,
    NotEqualBinary,
    NotInBinary,
    NotSameAsBinary,
    NullCoalesceBinary,
    ObjectDestructuringSetBinary,
    OrBinary,
    PowerBinary,
    RangeBinary,
    SameAsBinary,
    SequenceDestructuringSetBinary,
    SetBinary,
    SpaceshipBinary,
    StartsWithBinary,
    SubBinary,
    XorBinary,
    BlockReferenceExpression,
    CallExpression,
    ConditionalExpression,
    ConstantExpression,
    EmptyExpression,
    FilterExpression,
    DefaultFilter,
    EscapeFilter,
    RawFilter,
    FunctionExpression,
    EnumCasesFunction,
    EnumFunction,
    RenderSandboxedFunction,
    GetAttrExpression,
    InlinePrint,
    ListExpression,
    MacroReferenceExpression,
    MethodCallExpression,
    NameExpression,
    NullCoalesceExpression,
    ParentExpression,
    TempNameExpression,
    ConditionalTernary,
    TestExpression,
    ConstantTest,
    DefinedTest,
    DivisiblebyTest,
    EvenTest,
    NullTest,
    OddTest,
    SameasTest,
    TrueTest,
    AbstractUnary,
    NegUnary,
    NotUnary,
    PosUnary,
    SpreadUnary,
    StringCastUnary,
    AssignContextVariable,
    AssignMacroVariable,
    AssignTemplateVariable,
    ContextVariable,
    LocalVariable,
    MacroVariable,
    TemplateVariable,
    VariadicExpression,
    FlushNode,
    ForElseNode,
    ForLoopNode,
    ForNode,
    IfNode,
    ImportNode,
    IncludeNode,
    MacroDeclarationNode,
    MacroNode,
    MacrosNode,
    ModuleNode,
    NameDeprecation,
    Node,
    NodeDocumentation,
    Nodes,
    PrintNode,
    SandboxNode,
    SetNode,
    TextNode,
    TypeNode,
    TypesNode,
    WithNode,
}

/// The node interfaces of `Twig\Node` that classes implement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeInterface {
    CoercesChildrenToString,
    Binary,
    OperatorEscape,
    ReturnArray,
    ReturnBool,
    ReturnNumber,
    ReturnPrimitiveType,
    ReturnString,
    SupportDefinedTest,
    Unary,
    NodeCapture,
    NodeOutput,
}

impl NodeClass {
    /// The fully qualified PHP class name.
    pub fn php_class(self) -> &'static str {
        match self {
            Self::AutoEscapeNode => "Twig\\Node\\AutoEscapeNode",
            Self::BlockNode => "Twig\\Node\\BlockNode",
            Self::BlockReferenceNode => "Twig\\Node\\BlockReferenceNode",
            Self::BodyNode => "Twig\\Node\\BodyNode",
            Self::CaptureNode => "Twig\\Node\\CaptureNode",
            Self::CheckSecurityCallNode => "Twig\\Node\\CheckSecurityCallNode",
            Self::CheckSecurityNode => "Twig\\Node\\CheckSecurityNode",
            Self::CheckToStringNode => "Twig\\Node\\CheckToStringNode",
            Self::ConfigNode => "Twig\\Node\\ConfigNode",
            Self::DeprecatedNode => "Twig\\Node\\DeprecatedNode",
            Self::DoNode => "Twig\\Node\\DoNode",
            Self::EmbedNode => "Twig\\Node\\EmbedNode",
            Self::EmptyNode => "Twig\\Node\\EmptyNode",
            Self::AbstractExpression => "Twig\\Node\\Expression\\AbstractExpression",
            Self::ArrayExpression => "Twig\\Node\\Expression\\ArrayExpression",
            Self::ArrowFunctionExpression => "Twig\\Node\\Expression\\ArrowFunctionExpression",
            Self::AssignNameExpression => "Twig\\Node\\Expression\\AssignNameExpression",
            Self::AbstractBinary => "Twig\\Node\\Expression\\Binary\\AbstractBinary",
            Self::AddBinary => "Twig\\Node\\Expression\\Binary\\AddBinary",
            Self::AndBinary => "Twig\\Node\\Expression\\Binary\\AndBinary",
            Self::BitwiseAndBinary => "Twig\\Node\\Expression\\Binary\\BitwiseAndBinary",
            Self::BitwiseOrBinary => "Twig\\Node\\Expression\\Binary\\BitwiseOrBinary",
            Self::BitwiseXorBinary => "Twig\\Node\\Expression\\Binary\\BitwiseXorBinary",
            Self::ConcatBinary => "Twig\\Node\\Expression\\Binary\\ConcatBinary",
            Self::DivBinary => "Twig\\Node\\Expression\\Binary\\DivBinary",
            Self::ElvisBinary => "Twig\\Node\\Expression\\Binary\\ElvisBinary",
            Self::EndsWithBinary => "Twig\\Node\\Expression\\Binary\\EndsWithBinary",
            Self::EqualBinary => "Twig\\Node\\Expression\\Binary\\EqualBinary",
            Self::FloorDivBinary => "Twig\\Node\\Expression\\Binary\\FloorDivBinary",
            Self::GreaterBinary => "Twig\\Node\\Expression\\Binary\\GreaterBinary",
            Self::GreaterEqualBinary => "Twig\\Node\\Expression\\Binary\\GreaterEqualBinary",
            Self::HasEveryBinary => "Twig\\Node\\Expression\\Binary\\HasEveryBinary",
            Self::HasSomeBinary => "Twig\\Node\\Expression\\Binary\\HasSomeBinary",
            Self::InBinary => "Twig\\Node\\Expression\\Binary\\InBinary",
            Self::LessBinary => "Twig\\Node\\Expression\\Binary\\LessBinary",
            Self::LessEqualBinary => "Twig\\Node\\Expression\\Binary\\LessEqualBinary",
            Self::MatchesBinary => "Twig\\Node\\Expression\\Binary\\MatchesBinary",
            Self::ModBinary => "Twig\\Node\\Expression\\Binary\\ModBinary",
            Self::MulBinary => "Twig\\Node\\Expression\\Binary\\MulBinary",
            Self::NotEqualBinary => "Twig\\Node\\Expression\\Binary\\NotEqualBinary",
            Self::NotInBinary => "Twig\\Node\\Expression\\Binary\\NotInBinary",
            Self::NotSameAsBinary => "Twig\\Node\\Expression\\Binary\\NotSameAsBinary",
            Self::NullCoalesceBinary => "Twig\\Node\\Expression\\Binary\\NullCoalesceBinary",
            Self::ObjectDestructuringSetBinary => {
                "Twig\\Node\\Expression\\Binary\\ObjectDestructuringSetBinary"
            }
            Self::OrBinary => "Twig\\Node\\Expression\\Binary\\OrBinary",
            Self::PowerBinary => "Twig\\Node\\Expression\\Binary\\PowerBinary",
            Self::RangeBinary => "Twig\\Node\\Expression\\Binary\\RangeBinary",
            Self::SameAsBinary => "Twig\\Node\\Expression\\Binary\\SameAsBinary",
            Self::SequenceDestructuringSetBinary => {
                "Twig\\Node\\Expression\\Binary\\SequenceDestructuringSetBinary"
            }
            Self::SetBinary => "Twig\\Node\\Expression\\Binary\\SetBinary",
            Self::SpaceshipBinary => "Twig\\Node\\Expression\\Binary\\SpaceshipBinary",
            Self::StartsWithBinary => "Twig\\Node\\Expression\\Binary\\StartsWithBinary",
            Self::SubBinary => "Twig\\Node\\Expression\\Binary\\SubBinary",
            Self::XorBinary => "Twig\\Node\\Expression\\Binary\\XorBinary",
            Self::BlockReferenceExpression => "Twig\\Node\\Expression\\BlockReferenceExpression",
            Self::CallExpression => "Twig\\Node\\Expression\\CallExpression",
            Self::ConditionalExpression => "Twig\\Node\\Expression\\ConditionalExpression",
            Self::ConstantExpression => "Twig\\Node\\Expression\\ConstantExpression",
            Self::EmptyExpression => "Twig\\Node\\Expression\\EmptyExpression",
            Self::FilterExpression => "Twig\\Node\\Expression\\FilterExpression",
            Self::DefaultFilter => "Twig\\Node\\Expression\\Filter\\DefaultFilter",
            Self::EscapeFilter => "Twig\\Node\\Expression\\Filter\\EscapeFilter",
            Self::RawFilter => "Twig\\Node\\Expression\\Filter\\RawFilter",
            Self::FunctionExpression => "Twig\\Node\\Expression\\FunctionExpression",
            Self::EnumCasesFunction => "Twig\\Node\\Expression\\FunctionNode\\EnumCasesFunction",
            Self::EnumFunction => "Twig\\Node\\Expression\\FunctionNode\\EnumFunction",
            Self::RenderSandboxedFunction => {
                "Twig\\Node\\Expression\\FunctionNode\\RenderSandboxedFunction"
            }
            Self::GetAttrExpression => "Twig\\Node\\Expression\\GetAttrExpression",
            Self::InlinePrint => "Twig\\Node\\Expression\\InlinePrint",
            Self::ListExpression => "Twig\\Node\\Expression\\ListExpression",
            Self::MacroReferenceExpression => "Twig\\Node\\Expression\\MacroReferenceExpression",
            Self::MethodCallExpression => "Twig\\Node\\Expression\\MethodCallExpression",
            Self::NameExpression => "Twig\\Node\\Expression\\NameExpression",
            Self::NullCoalesceExpression => "Twig\\Node\\Expression\\NullCoalesceExpression",
            Self::ParentExpression => "Twig\\Node\\Expression\\ParentExpression",
            Self::TempNameExpression => "Twig\\Node\\Expression\\TempNameExpression",
            Self::ConditionalTernary => "Twig\\Node\\Expression\\Ternary\\ConditionalTernary",
            Self::TestExpression => "Twig\\Node\\Expression\\TestExpression",
            Self::ConstantTest => "Twig\\Node\\Expression\\Test\\ConstantTest",
            Self::DefinedTest => "Twig\\Node\\Expression\\Test\\DefinedTest",
            Self::DivisiblebyTest => "Twig\\Node\\Expression\\Test\\DivisiblebyTest",
            Self::EvenTest => "Twig\\Node\\Expression\\Test\\EvenTest",
            Self::NullTest => "Twig\\Node\\Expression\\Test\\NullTest",
            Self::OddTest => "Twig\\Node\\Expression\\Test\\OddTest",
            Self::SameasTest => "Twig\\Node\\Expression\\Test\\SameasTest",
            Self::TrueTest => "Twig\\Node\\Expression\\Test\\TrueTest",
            Self::AbstractUnary => "Twig\\Node\\Expression\\Unary\\AbstractUnary",
            Self::NegUnary => "Twig\\Node\\Expression\\Unary\\NegUnary",
            Self::NotUnary => "Twig\\Node\\Expression\\Unary\\NotUnary",
            Self::PosUnary => "Twig\\Node\\Expression\\Unary\\PosUnary",
            Self::SpreadUnary => "Twig\\Node\\Expression\\Unary\\SpreadUnary",
            Self::StringCastUnary => "Twig\\Node\\Expression\\Unary\\StringCastUnary",
            Self::AssignContextVariable => {
                "Twig\\Node\\Expression\\Variable\\AssignContextVariable"
            }
            Self::AssignMacroVariable => "Twig\\Node\\Expression\\Variable\\AssignMacroVariable",
            Self::AssignTemplateVariable => {
                "Twig\\Node\\Expression\\Variable\\AssignTemplateVariable"
            }
            Self::ContextVariable => "Twig\\Node\\Expression\\Variable\\ContextVariable",
            Self::LocalVariable => "Twig\\Node\\Expression\\Variable\\LocalVariable",
            Self::MacroVariable => "Twig\\Node\\Expression\\Variable\\MacroVariable",
            Self::TemplateVariable => "Twig\\Node\\Expression\\Variable\\TemplateVariable",
            Self::VariadicExpression => "Twig\\Node\\Expression\\VariadicExpression",
            Self::FlushNode => "Twig\\Node\\FlushNode",
            Self::ForElseNode => "Twig\\Node\\ForElseNode",
            Self::ForLoopNode => "Twig\\Node\\ForLoopNode",
            Self::ForNode => "Twig\\Node\\ForNode",
            Self::IfNode => "Twig\\Node\\IfNode",
            Self::ImportNode => "Twig\\Node\\ImportNode",
            Self::IncludeNode => "Twig\\Node\\IncludeNode",
            Self::MacroDeclarationNode => "Twig\\Node\\MacroDeclarationNode",
            Self::MacroNode => "Twig\\Node\\MacroNode",
            Self::MacrosNode => "Twig\\Node\\MacrosNode",
            Self::ModuleNode => "Twig\\Node\\ModuleNode",
            Self::NameDeprecation => "Twig\\Node\\NameDeprecation",
            Self::Node => "Twig\\Node\\Node",
            Self::NodeDocumentation => "Twig\\Node\\NodeDocumentation",
            Self::Nodes => "Twig\\Node\\Nodes",
            Self::PrintNode => "Twig\\Node\\PrintNode",
            Self::SandboxNode => "Twig\\Node\\SandboxNode",
            Self::SetNode => "Twig\\Node\\SetNode",
            Self::TextNode => "Twig\\Node\\TextNode",
            Self::TypeNode => "Twig\\Node\\TypeNode",
            Self::TypesNode => "Twig\\Node\\TypesNode",
            Self::WithNode => "Twig\\Node\\WithNode",
        }
    }

    /// Whether the class is abstract.
    pub fn is_abstract(self) -> bool {
        matches!(
            self,
            Self::AbstractExpression
                | Self::AbstractBinary
                | Self::CallExpression
                | Self::AbstractUnary
        )
    }

    /// The parent class.
    pub fn parent(self) -> Option<Self> {
        match self {
            Self::AutoEscapeNode => Some(Self::Node),
            Self::BlockNode => Some(Self::Node),
            Self::BlockReferenceNode => Some(Self::Node),
            Self::BodyNode => Some(Self::Node),
            Self::CaptureNode => Some(Self::Node),
            Self::CheckSecurityCallNode => Some(Self::Node),
            Self::CheckSecurityNode => Some(Self::Node),
            Self::CheckToStringNode => Some(Self::AbstractExpression),
            Self::ConfigNode => Some(Self::Node),
            Self::DeprecatedNode => Some(Self::Node),
            Self::DoNode => Some(Self::Node),
            Self::EmbedNode => Some(Self::IncludeNode),
            Self::EmptyNode => Some(Self::Node),
            Self::AbstractExpression => Some(Self::Node),
            Self::ArrayExpression => Some(Self::AbstractExpression),
            Self::ArrowFunctionExpression => Some(Self::AbstractExpression),
            Self::AssignNameExpression => Some(Self::ContextVariable),
            Self::AbstractBinary => Some(Self::AbstractExpression),
            Self::AddBinary => Some(Self::AbstractBinary),
            Self::AndBinary => Some(Self::AbstractBinary),
            Self::BitwiseAndBinary => Some(Self::AbstractBinary),
            Self::BitwiseOrBinary => Some(Self::AbstractBinary),
            Self::BitwiseXorBinary => Some(Self::AbstractBinary),
            Self::ConcatBinary => Some(Self::AbstractBinary),
            Self::DivBinary => Some(Self::AbstractBinary),
            Self::ElvisBinary => Some(Self::AbstractBinary),
            Self::EndsWithBinary => Some(Self::AbstractBinary),
            Self::EqualBinary => Some(Self::AbstractBinary),
            Self::FloorDivBinary => Some(Self::AbstractBinary),
            Self::GreaterBinary => Some(Self::AbstractBinary),
            Self::GreaterEqualBinary => Some(Self::AbstractBinary),
            Self::HasEveryBinary => Some(Self::AbstractBinary),
            Self::HasSomeBinary => Some(Self::AbstractBinary),
            Self::InBinary => Some(Self::AbstractBinary),
            Self::LessBinary => Some(Self::AbstractBinary),
            Self::LessEqualBinary => Some(Self::AbstractBinary),
            Self::MatchesBinary => Some(Self::AbstractBinary),
            Self::ModBinary => Some(Self::AbstractBinary),
            Self::MulBinary => Some(Self::AbstractBinary),
            Self::NotEqualBinary => Some(Self::AbstractBinary),
            Self::NotInBinary => Some(Self::AbstractBinary),
            Self::NotSameAsBinary => Some(Self::AbstractBinary),
            Self::NullCoalesceBinary => Some(Self::AbstractBinary),
            Self::ObjectDestructuringSetBinary => Some(Self::AbstractBinary),
            Self::OrBinary => Some(Self::AbstractBinary),
            Self::PowerBinary => Some(Self::AbstractBinary),
            Self::RangeBinary => Some(Self::AbstractBinary),
            Self::SameAsBinary => Some(Self::AbstractBinary),
            Self::SequenceDestructuringSetBinary => Some(Self::AbstractBinary),
            Self::SetBinary => Some(Self::AbstractBinary),
            Self::SpaceshipBinary => Some(Self::AbstractBinary),
            Self::StartsWithBinary => Some(Self::AbstractBinary),
            Self::SubBinary => Some(Self::AbstractBinary),
            Self::XorBinary => Some(Self::AbstractBinary),
            Self::BlockReferenceExpression => Some(Self::AbstractExpression),
            Self::CallExpression => Some(Self::AbstractExpression),
            Self::ConditionalExpression => Some(Self::AbstractExpression),
            Self::ConstantExpression => Some(Self::AbstractExpression),
            Self::EmptyExpression => Some(Self::AbstractExpression),
            Self::FilterExpression => Some(Self::CallExpression),
            Self::DefaultFilter => Some(Self::FilterExpression),
            Self::EscapeFilter => Some(Self::FilterExpression),
            Self::RawFilter => Some(Self::FilterExpression),
            Self::FunctionExpression => Some(Self::CallExpression),
            Self::EnumCasesFunction => Some(Self::FunctionExpression),
            Self::EnumFunction => Some(Self::FunctionExpression),
            Self::RenderSandboxedFunction => Some(Self::FunctionExpression),
            Self::GetAttrExpression => Some(Self::AbstractExpression),
            Self::InlinePrint => Some(Self::AbstractExpression),
            Self::ListExpression => Some(Self::AbstractExpression),
            Self::MacroReferenceExpression => Some(Self::AbstractExpression),
            Self::MethodCallExpression => Some(Self::AbstractExpression),
            Self::NameExpression => Some(Self::AbstractExpression),
            Self::NullCoalesceExpression => Some(Self::ConditionalExpression),
            Self::ParentExpression => Some(Self::AbstractExpression),
            Self::TempNameExpression => Some(Self::AbstractExpression),
            Self::ConditionalTernary => Some(Self::AbstractExpression),
            Self::TestExpression => Some(Self::CallExpression),
            Self::ConstantTest => Some(Self::TestExpression),
            Self::DefinedTest => Some(Self::TestExpression),
            Self::DivisiblebyTest => Some(Self::TestExpression),
            Self::EvenTest => Some(Self::TestExpression),
            Self::NullTest => Some(Self::TestExpression),
            Self::OddTest => Some(Self::TestExpression),
            Self::SameasTest => Some(Self::TestExpression),
            Self::TrueTest => Some(Self::TestExpression),
            Self::AbstractUnary => Some(Self::AbstractExpression),
            Self::NegUnary => Some(Self::AbstractUnary),
            Self::NotUnary => Some(Self::AbstractUnary),
            Self::PosUnary => Some(Self::AbstractUnary),
            Self::SpreadUnary => Some(Self::AbstractUnary),
            Self::StringCastUnary => Some(Self::AbstractUnary),
            Self::AssignContextVariable => Some(Self::AssignNameExpression),
            Self::AssignMacroVariable => Some(Self::AbstractExpression),
            Self::AssignTemplateVariable => Some(Self::AssignMacroVariable),
            Self::ContextVariable => Some(Self::NameExpression),
            Self::LocalVariable => Some(Self::TempNameExpression),
            Self::MacroVariable => Some(Self::TempNameExpression),
            Self::TemplateVariable => Some(Self::MacroVariable),
            Self::VariadicExpression => Some(Self::ArrayExpression),
            Self::FlushNode => Some(Self::Node),
            Self::ForElseNode => Some(Self::Node),
            Self::ForLoopNode => Some(Self::Node),
            Self::ForNode => Some(Self::Node),
            Self::IfNode => Some(Self::Node),
            Self::ImportNode => Some(Self::Node),
            Self::IncludeNode => Some(Self::Node),
            Self::MacroDeclarationNode => Some(Self::ConfigNode),
            Self::MacroNode => Some(Self::Node),
            Self::MacrosNode => Some(Self::Node),
            Self::ModuleNode => Some(Self::Node),
            Self::NameDeprecation => None,
            Self::Node => None,
            Self::NodeDocumentation => None,
            Self::Nodes => Some(Self::Node),
            Self::PrintNode => Some(Self::Node),
            Self::SandboxNode => Some(Self::Node),
            Self::SetNode => Some(Self::Node),
            Self::TextNode => Some(Self::Node),
            Self::TypeNode => Some(Self::Node),
            Self::TypesNode => Some(Self::Node),
            Self::WithNode => Some(Self::Node),
        }
    }

    /// `instanceof` another class: this class or one of its parents.
    pub fn is_a(self, class: Self) -> bool {
        let mut c = Some(self);
        while let Some(current) = c {
            if current == class {
                return true;
            }
            c = current.parent();
        }
        false
    }

    /// `instanceof` an interface.
    pub fn implements(self, interface: NodeInterface) -> bool {
        match interface {
            NodeInterface::CoercesChildrenToString => matches!(
                self,
                Self::DeprecatedNode
                    | Self::EmbedNode
                    | Self::ArrayExpression
                    | Self::ConcatBinary
                    | Self::EqualBinary
                    | Self::GreaterBinary
                    | Self::GreaterEqualBinary
                    | Self::InBinary
                    | Self::LessBinary
                    | Self::LessEqualBinary
                    | Self::MatchesBinary
                    | Self::NotEqualBinary
                    | Self::NotInBinary
                    | Self::RangeBinary
                    | Self::SpaceshipBinary
                    | Self::BlockReferenceExpression
                    | Self::FilterExpression
                    | Self::DefaultFilter
                    | Self::EscapeFilter
                    | Self::RawFilter
                    | Self::FunctionExpression
                    | Self::EnumCasesFunction
                    | Self::EnumFunction
                    | Self::RenderSandboxedFunction
                    | Self::GetAttrExpression
                    | Self::MacroReferenceExpression
                    | Self::TestExpression
                    | Self::ConstantTest
                    | Self::DefinedTest
                    | Self::DivisiblebyTest
                    | Self::EvenTest
                    | Self::NullTest
                    | Self::OddTest
                    | Self::SameasTest
                    | Self::TrueTest
                    | Self::VariadicExpression
                    | Self::ImportNode
                    | Self::IncludeNode
                    | Self::ModuleNode
                    | Self::PrintNode
            ),
            NodeInterface::Binary => matches!(
                self,
                Self::AbstractBinary
                    | Self::AddBinary
                    | Self::AndBinary
                    | Self::BitwiseAndBinary
                    | Self::BitwiseOrBinary
                    | Self::BitwiseXorBinary
                    | Self::ConcatBinary
                    | Self::DivBinary
                    | Self::ElvisBinary
                    | Self::EndsWithBinary
                    | Self::EqualBinary
                    | Self::FloorDivBinary
                    | Self::GreaterBinary
                    | Self::GreaterEqualBinary
                    | Self::HasEveryBinary
                    | Self::HasSomeBinary
                    | Self::InBinary
                    | Self::LessBinary
                    | Self::LessEqualBinary
                    | Self::MatchesBinary
                    | Self::ModBinary
                    | Self::MulBinary
                    | Self::NotEqualBinary
                    | Self::NotInBinary
                    | Self::NotSameAsBinary
                    | Self::NullCoalesceBinary
                    | Self::ObjectDestructuringSetBinary
                    | Self::OrBinary
                    | Self::PowerBinary
                    | Self::RangeBinary
                    | Self::SameAsBinary
                    | Self::SequenceDestructuringSetBinary
                    | Self::SetBinary
                    | Self::SpaceshipBinary
                    | Self::StartsWithBinary
                    | Self::SubBinary
                    | Self::XorBinary
            ),
            NodeInterface::OperatorEscape => matches!(
                self,
                Self::ElvisBinary
                    | Self::NullCoalesceBinary
                    | Self::ConditionalExpression
                    | Self::NullCoalesceExpression
                    | Self::ConditionalTernary
            ),
            NodeInterface::ReturnArray => matches!(
                self,
                Self::ArrayExpression | Self::RangeBinary | Self::VariadicExpression
            ),
            NodeInterface::ReturnBool => matches!(
                self,
                Self::AndBinary
                    | Self::EndsWithBinary
                    | Self::EqualBinary
                    | Self::GreaterBinary
                    | Self::GreaterEqualBinary
                    | Self::HasEveryBinary
                    | Self::HasSomeBinary
                    | Self::InBinary
                    | Self::LessBinary
                    | Self::LessEqualBinary
                    | Self::MatchesBinary
                    | Self::NotEqualBinary
                    | Self::NotInBinary
                    | Self::NotSameAsBinary
                    | Self::OrBinary
                    | Self::SameAsBinary
                    | Self::StartsWithBinary
                    | Self::XorBinary
                    | Self::TestExpression
                    | Self::ConstantTest
                    | Self::DefinedTest
                    | Self::DivisiblebyTest
                    | Self::EvenTest
                    | Self::NullTest
                    | Self::OddTest
                    | Self::SameasTest
                    | Self::TrueTest
                    | Self::NotUnary
            ),
            NodeInterface::ReturnNumber => matches!(
                self,
                Self::AddBinary
                    | Self::BitwiseAndBinary
                    | Self::BitwiseOrBinary
                    | Self::BitwiseXorBinary
                    | Self::DivBinary
                    | Self::FloorDivBinary
                    | Self::ModBinary
                    | Self::MulBinary
                    | Self::PowerBinary
                    | Self::SpaceshipBinary
                    | Self::SubBinary
            ),
            NodeInterface::ReturnPrimitiveType => matches!(
                self,
                Self::ArrayExpression
                    | Self::AddBinary
                    | Self::AndBinary
                    | Self::BitwiseAndBinary
                    | Self::BitwiseOrBinary
                    | Self::BitwiseXorBinary
                    | Self::ConcatBinary
                    | Self::DivBinary
                    | Self::EndsWithBinary
                    | Self::EqualBinary
                    | Self::FloorDivBinary
                    | Self::GreaterBinary
                    | Self::GreaterEqualBinary
                    | Self::HasEveryBinary
                    | Self::HasSomeBinary
                    | Self::InBinary
                    | Self::LessBinary
                    | Self::LessEqualBinary
                    | Self::MatchesBinary
                    | Self::ModBinary
                    | Self::MulBinary
                    | Self::NotEqualBinary
                    | Self::NotInBinary
                    | Self::NotSameAsBinary
                    | Self::OrBinary
                    | Self::PowerBinary
                    | Self::RangeBinary
                    | Self::SameAsBinary
                    | Self::SpaceshipBinary
                    | Self::StartsWithBinary
                    | Self::SubBinary
                    | Self::XorBinary
                    | Self::ConstantExpression
                    | Self::TestExpression
                    | Self::ConstantTest
                    | Self::DefinedTest
                    | Self::DivisiblebyTest
                    | Self::EvenTest
                    | Self::NullTest
                    | Self::OddTest
                    | Self::SameasTest
                    | Self::TrueTest
                    | Self::NotUnary
                    | Self::VariadicExpression
            ),
            NodeInterface::ReturnString => matches!(self, Self::ConcatBinary),
            NodeInterface::SupportDefinedTest => matches!(
                self,
                Self::ArrayExpression
                    | Self::AssignNameExpression
                    | Self::BlockReferenceExpression
                    | Self::ConstantExpression
                    | Self::FunctionExpression
                    | Self::EnumCasesFunction
                    | Self::EnumFunction
                    | Self::RenderSandboxedFunction
                    | Self::GetAttrExpression
                    | Self::MacroReferenceExpression
                    | Self::MethodCallExpression
                    | Self::NameExpression
                    | Self::AssignContextVariable
                    | Self::ContextVariable
                    | Self::VariadicExpression
            ),
            NodeInterface::Unary => matches!(
                self,
                Self::AbstractUnary
                    | Self::NegUnary
                    | Self::NotUnary
                    | Self::PosUnary
                    | Self::SpreadUnary
                    | Self::StringCastUnary
            ),
            NodeInterface::NodeCapture => matches!(self, Self::SetNode),
            NodeInterface::NodeOutput => matches!(
                self,
                Self::BlockReferenceNode
                    | Self::EmbedNode
                    | Self::IncludeNode
                    | Self::PrintNode
                    | Self::TextNode
            ),
        }
    }
}
