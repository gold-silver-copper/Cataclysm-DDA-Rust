use serde_json::Value;

use crate::eoc::EocActorStatDefinition;

pub const MAX_EOC_MATH_NODES: usize = 256;
pub const MAX_EOC_MATH_SOURCE_BYTES: usize = 4_096;
pub const MAX_EOC_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EocMathExpressionDefinition {
    Constant(i64),
    ActorVariable(String),
    HasActorVariable(String),
    ActorStat(EocActorStatDefinition),
    Negate(Box<Self>),
    Not(Box<Self>),
    Add(Box<Self>, Box<Self>),
    Subtract(Box<Self>, Box<Self>),
    Multiply(Box<Self>, Box<Self>),
    Equal(Box<Self>, Box<Self>),
    NotEqual(Box<Self>, Box<Self>),
    Less(Box<Self>, Box<Self>),
    LessOrEqual(Box<Self>, Box<Self>),
    Greater(Box<Self>, Box<Self>),
    GreaterOrEqual(Box<Self>, Box<Self>),
    And(Box<Self>, Box<Self>),
    Or(Box<Self>, Box<Self>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EocMathAssignmentOperationDefinition {
    Set,
    Add,
    Subtract,
    Multiply,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EocMathAssignmentDefinition {
    pub variable_id: String,
    pub operation: EocMathAssignmentOperationDefinition,
    pub value: EocMathExpressionDefinition,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Token {
    Number(i64),
    Identifier(String),
    Text(String),
    LeftParen,
    RightParen,
    Plus,
    Minus,
    Star,
    Bang,
    PlusPlus,
    MinusMinus,
    Assign,
    AddAssign,
    SubtractAssign,
    MultiplyAssign,
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    And,
    Or,
}

pub(crate) fn parse_math_condition(value: &Value) -> Option<EocMathExpressionDefinition> {
    let source = math_source(value)?;
    let tokens = lex(source)?;
    let mut parser = Parser::new(tokens);
    let expression = parser.parse_expression()?;
    parser.finished().then_some(expression)
}

pub(crate) fn parse_math_assignment(value: &Value) -> Option<EocMathAssignmentDefinition> {
    let source = math_source(value)?;
    let tokens = lex(source)?;
    let mut parser = Parser::new(tokens);
    let variable_id = parser.take_actor_variable()?;
    if parser.take_if(&Token::PlusPlus) {
        return parser.finished().then_some(EocMathAssignmentDefinition {
            variable_id,
            operation: EocMathAssignmentOperationDefinition::Add,
            value: EocMathExpressionDefinition::Constant(1),
        });
    }
    if parser.take_if(&Token::MinusMinus) {
        return parser.finished().then_some(EocMathAssignmentDefinition {
            variable_id,
            operation: EocMathAssignmentOperationDefinition::Subtract,
            value: EocMathExpressionDefinition::Constant(1),
        });
    }
    let operation = match parser.take()? {
        Token::Assign => EocMathAssignmentOperationDefinition::Set,
        Token::AddAssign => EocMathAssignmentOperationDefinition::Add,
        Token::SubtractAssign => EocMathAssignmentOperationDefinition::Subtract,
        Token::MultiplyAssign => EocMathAssignmentOperationDefinition::Multiply,
        _ => return None,
    };
    let value = parser.parse_expression()?;
    parser.finished().then_some(EocMathAssignmentDefinition {
        variable_id,
        operation,
        value,
    })
}

fn math_source(value: &Value) -> Option<&str> {
    let values = value.as_array()?;
    let [source] = values.as_slice() else {
        return None;
    };
    source
        .as_str()
        .filter(|source| !source.is_empty() && source.len() <= MAX_EOC_MATH_SOURCE_BYTES)
}

fn actor_variable_id(identifier: &str) -> Option<String> {
    let id = identifier.strip_prefix("u_")?;
    (!id.is_empty() && id.len() <= 512).then(|| id.to_owned())
}

fn lex(source: &str) -> Option<Vec<Token>> {
    if !source.is_ascii() {
        return None;
    }
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if byte.is_ascii_digit() {
            let start = index;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
            let value = source[start..index].parse::<i64>().ok()?;
            if value > MAX_EOC_SAFE_INTEGER {
                return None;
            }
            tokens.push(Token::Number(value));
            continue;
        }
        if byte.is_ascii_alphabetic() || byte == b'_' {
            let start = index;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            tokens.push(Token::Identifier(source[start..index].to_owned()));
            continue;
        }
        if matches!(byte, b'\'' | b'"') {
            let quote = byte;
            index += 1;
            let start = index;
            while index < bytes.len() && bytes[index] != quote {
                if bytes[index] == b'\\' || bytes[index].is_ascii_control() {
                    return None;
                }
                index += 1;
            }
            if index == bytes.len() {
                return None;
            }
            tokens.push(Token::Text(source[start..index].to_owned()));
            index += 1;
            continue;
        }
        let two = bytes.get(index..index + 2);
        let pair = match two {
            Some(b"++") => Some(Token::PlusPlus),
            Some(b"--") => Some(Token::MinusMinus),
            Some(b"+=") => Some(Token::AddAssign),
            Some(b"-=") => Some(Token::SubtractAssign),
            Some(b"*=") => Some(Token::MultiplyAssign),
            Some(b"==") => Some(Token::Equal),
            Some(b"!=") => Some(Token::NotEqual),
            Some(b"<=") => Some(Token::LessOrEqual),
            Some(b">=") => Some(Token::GreaterOrEqual),
            Some(b"&&") => Some(Token::And),
            Some(b"||") => Some(Token::Or),
            _ => None,
        };
        if let Some(token) = pair {
            tokens.push(token);
            index += 2;
            continue;
        }
        tokens.push(match byte {
            b'(' => Token::LeftParen,
            b')' => Token::RightParen,
            b'+' => Token::Plus,
            b'-' => Token::Minus,
            b'*' => Token::Star,
            b'!' => Token::Bang,
            b'=' => Token::Assign,
            b'<' => Token::Less,
            b'>' => Token::Greater,
            _ => return None,
        });
        index += 1;
    }
    (!tokens.is_empty()).then_some(tokens)
}

struct Parser {
    tokens: Vec<Token>,
    position: usize,
    nodes: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            position: 0,
            nodes: 0,
        }
    }

    fn finished(&self) -> bool {
        self.position == self.tokens.len()
    }

    fn take(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.position)?.clone();
        self.position += 1;
        Some(token)
    }

    fn take_if(&mut self, expected: &Token) -> bool {
        if self.tokens.get(self.position) == Some(expected) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn node(
        &mut self,
        expression: EocMathExpressionDefinition,
    ) -> Option<EocMathExpressionDefinition> {
        self.nodes = self.nodes.checked_add(1)?;
        (self.nodes <= MAX_EOC_MATH_NODES).then_some(expression)
    }

    fn take_actor_variable(&mut self) -> Option<String> {
        let Token::Identifier(identifier) = self.take()? else {
            return None;
        };
        actor_variable_id(&identifier)
    }

    fn parse_expression(&mut self) -> Option<EocMathExpressionDefinition> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Option<EocMathExpressionDefinition> {
        let mut left = self.parse_and()?;
        while self.take_if(&Token::Or) {
            let right = self.parse_and()?;
            left = self.node(EocMathExpressionDefinition::Or(
                Box::new(left),
                Box::new(right),
            ))?;
        }
        Some(left)
    }

    fn parse_and(&mut self) -> Option<EocMathExpressionDefinition> {
        let mut left = self.parse_comparison()?;
        while self.take_if(&Token::And) {
            let right = self.parse_comparison()?;
            left = self.node(EocMathExpressionDefinition::And(
                Box::new(left),
                Box::new(right),
            ))?;
        }
        Some(left)
    }

    fn parse_comparison(&mut self) -> Option<EocMathExpressionDefinition> {
        let left = self.parse_additive()?;
        let constructor = match self.tokens.get(self.position) {
            Some(Token::Equal) => EocMathExpressionDefinition::Equal,
            Some(Token::NotEqual) => EocMathExpressionDefinition::NotEqual,
            Some(Token::Less) => EocMathExpressionDefinition::Less,
            Some(Token::LessOrEqual) => EocMathExpressionDefinition::LessOrEqual,
            Some(Token::Greater) => EocMathExpressionDefinition::Greater,
            Some(Token::GreaterOrEqual) => EocMathExpressionDefinition::GreaterOrEqual,
            _ => return Some(left),
        };
        self.position += 1;
        let right = self.parse_additive()?;
        self.node(constructor(Box::new(left), Box::new(right)))
    }

    fn parse_additive(&mut self) -> Option<EocMathExpressionDefinition> {
        let mut left = self.parse_multiplicative()?;
        loop {
            let constructor = match self.tokens.get(self.position) {
                Some(Token::Plus) => EocMathExpressionDefinition::Add,
                Some(Token::Minus) => EocMathExpressionDefinition::Subtract,
                _ => return Some(left),
            };
            self.position += 1;
            let right = self.parse_multiplicative()?;
            left = self.node(constructor(Box::new(left), Box::new(right)))?;
        }
    }

    fn parse_multiplicative(&mut self) -> Option<EocMathExpressionDefinition> {
        let mut left = self.parse_unary()?;
        while self.take_if(&Token::Star) {
            let right = self.parse_unary()?;
            left = self.node(EocMathExpressionDefinition::Multiply(
                Box::new(left),
                Box::new(right),
            ))?;
        }
        Some(left)
    }

    fn parse_unary(&mut self) -> Option<EocMathExpressionDefinition> {
        if self.take_if(&Token::Minus) {
            let value = self.parse_unary()?;
            return self.node(EocMathExpressionDefinition::Negate(Box::new(value)));
        }
        if self.take_if(&Token::Plus) {
            return self.parse_unary();
        }
        if self.take_if(&Token::Bang) {
            let value = self.parse_unary()?;
            return self.node(EocMathExpressionDefinition::Not(Box::new(value)));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Option<EocMathExpressionDefinition> {
        match self.take()? {
            Token::Number(value) => self.node(EocMathExpressionDefinition::Constant(value)),
            Token::Identifier(identifier) if self.take_if(&Token::LeftParen) => {
                self.parse_function(&identifier)
            }
            Token::Identifier(identifier) => self.node(EocMathExpressionDefinition::ActorVariable(
                actor_variable_id(&identifier)?,
            )),
            Token::LeftParen => {
                let expression = self.parse_expression()?;
                self.take_if(&Token::RightParen).then_some(expression)
            }
            _ => None,
        }
    }

    fn parse_function(&mut self, function: &str) -> Option<EocMathExpressionDefinition> {
        let expression = match function {
            "has_var" => {
                let variable_id = self.take_actor_variable()?;
                EocMathExpressionDefinition::HasActorVariable(variable_id)
            }
            "u_val" => {
                let stat = match self.take()? {
                    Token::Text(stat) | Token::Identifier(stat) => stat,
                    _ => return None,
                };
                EocMathExpressionDefinition::ActorStat(match stat.as_str() {
                    "strength" => EocActorStatDefinition::Strength,
                    "dexterity" => EocActorStatDefinition::Dexterity,
                    "intelligence" => EocActorStatDefinition::Intelligence,
                    "perception" => EocActorStatDefinition::Perception,
                    _ => return None,
                })
            }
            _ => return None,
        };
        if !self.take_if(&Token::RightParen) {
            return None;
        }
        self.node(expression)
    }
}
