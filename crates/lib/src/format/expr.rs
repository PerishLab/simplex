use super::Printer;
use crate::tree::{Body, Expression, Field, Parameter, Spec};

impl Printer {
    pub(crate) fn expression(&mut self, expr: &Expression) {
        match expr {
            Expression::Word(text)
            | Expression::Number(text)
            | Expression::Text(text)
            | Expression::Byte(text) => self.out.push_str(text),
            Expression::Group(inner) => {
                self.out.push('(');
                self.expression(inner);
                self.out.push(')');
            }
            Expression::Unary { operator, operand } => {
                self.out.push_str(operator);
                self.expression(operand);
            }
            Expression::Binary {
                left,
                operator,
                right,
                wrapped,
            } => self.binary(left, operator, right, *wrapped),
            Expression::Member {
                base,
                field,
                generics,
                wrapped,
            } => self.member(base, field, generics, *wrapped),
            Expression::Call {
                callee,
                arguments,
                multiline,
            } => self.call(callee, arguments, *multiline),
            Expression::Record {
                name,
                fields,
                multiline,
            } => self.record(name, fields, *multiline),
            Expression::Try(inner) => {
                self.expression(inner);
                self.out.push('?');
            }
            Expression::Switch(dispatch) => self.dispatch(dispatch),
            Expression::Bucket(partition) => self.partition(partition),
            Expression::Loop(kind) => self.repetition(kind),
            Expression::Function {
                inputs,
                output,
                body,
            } => self.function(inputs, output.as_ref(), body),
        }
    }

    fn binary(&mut self, left: &Expression, operator: &str, right: &Expression, wrapped: bool) {
        self.expression(left);
        self.out.push(' ');
        self.out.push_str(operator);
        if wrapped {
            self.out.push('\n');
            self.indent += 1;
            self.margin();
            self.expression(right);
            self.indent -= 1;
        } else {
            self.out.push(' ');
            self.expression(right);
        }
    }

    fn member(&mut self, base: &Expression, field: &str, generics: &[Spec], wrapped: bool) {
        self.expression(base);
        self.out.push('.');
        if wrapped {
            self.out.push('\n');
            self.indent += 1;
            self.margin();
        }
        self.out.push_str(field);
        if !generics.is_empty() {
            self.out.push('<');
            for (index, spec) in generics.iter().enumerate() {
                if index > 0 {
                    self.out.push_str(", ");
                }
                self.spec(spec);
            }
            self.out.push('>');
        }
        if wrapped {
            self.indent -= 1;
        }
    }

    fn call(&mut self, callee: &Expression, arguments: &[Expression], multiline: bool) {
        self.expression(callee);
        self.out.push('(');
        if multiline && !arguments.is_empty() {
            self.out.push('\n');
            self.indent += 1;
            for arg in arguments {
                self.margin();
                self.expression(arg);
                self.out.push_str(",\n");
            }
            self.indent -= 1;
            self.margin();
        } else {
            for (index, arg) in arguments.iter().enumerate() {
                if index > 0 {
                    self.out.push_str(", ");
                }
                self.expression(arg);
            }
        }
        self.out.push(')');
    }

    fn record(&mut self, name: &str, fields: &[Field], multiline: bool) {
        self.out.push_str(name);
        if fields.is_empty() {
            self.out.push_str(" {}");
            return;
        }
        if multiline {
            self.out.push_str(" {\n");
            self.indent += 1;
            for field in fields {
                self.margin();
                self.out.push_str(&field.name);
                self.out.push_str(": ");
                self.expression(&field.value);
                self.out.push_str(",\n");
            }
            self.indent -= 1;
            self.margin();
            self.out.push('}');
        } else {
            self.out.push_str(" { ");
            for (index, field) in fields.iter().enumerate() {
                if index > 0 {
                    self.out.push_str(", ");
                }
                self.out.push_str(&field.name);
                self.out.push_str(": ");
                self.expression(&field.value);
            }
            self.out.push_str(" }");
        }
    }

    fn function(&mut self, inputs: &[Parameter], output: Option<&Spec>, body: &Body) {
        self.parameters(inputs);
        if let Some(spec) = output {
            self.out.push_str(" -> ");
            self.spec(spec);
        }
        self.out.push(' ');
        match body {
            Body::Block(block) => self.block(block),
            Body::Switch(dispatch) => self.dispatch(dispatch),
            Body::Bucket(partition) => self.partition(partition),
            Body::Loop(kind) => self.repetition(kind),
        }
    }
}
