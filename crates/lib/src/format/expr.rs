use super::Printer;
use crate::tree::{Body, Expression, Parameter, Spec};

impl Printer {
    pub(crate) fn expression(&mut self, expr: &Expression) {
        match expr {
            Expression::Word { text, .. }
            | Expression::Number(text)
            | Expression::Text(text)
            | Expression::Byte(text) => self.out.push_str(text),
            Expression::Group(inner) => {
                self.out.push('(');
                self.expression(inner);
                self.out.push(')');
            }
            Expression::List {
                items, multiline, ..
            } => self.list(items, *multiline),
            Expression::Unary { operator, operand } => {
                self.out.push_str(operator);
                self.expression(operand);
            }
            Expression::Borrow {
                mutable, target, ..
            } => {
                if *mutable {
                    self.out.push_str("&var ");
                } else {
                    self.out.push('&');
                }
                self.expression(target);
            }
            Expression::Index {
                base, start, limit, ..
            } => self.index(base, start, limit.as_deref()),
            Expression::Binary {
                left,
                operator,
                right,
                wrapped,
                ..
            } => self.binary(left, operator, right, *wrapped),
            Expression::Member {
                base,
                field,
                generics,
                wrapped,
                ..
            } => self.member(base, field, generics, *wrapped),
            Expression::Call { .. } => self.call(expr),
            Expression::Meta(block) => {
                self.out.push_str("comptime ");
                self.block(block);
            }
            Expression::Record { .. } => self.record(expr),
            Expression::Try(inner) => {
                self.expression(inner);
                self.out.push('?');
            }
            Expression::Switch(dispatch) => self.dispatch(dispatch),
            Expression::Bucket(partition) => self.partition(partition),
            Expression::Loop(kind) => self.repetition(kind),
            Expression::Function {
                asynchronous,
                generics,
                inputs,
                output,
                body,
            } => {
                if *asynchronous {
                    self.out.push_str("async ");
                }
                self.clauses(generics);
                self.function(inputs, output.as_ref(), body);
            }
        }
    }

    fn list(&mut self, items: &[Expression], multiline: bool) {
        self.out.push('[');
        if multiline && !items.is_empty() {
            self.out.push('\n');
            self.indent += 1;
            for item in items {
                self.margin();
                self.expression(item);
                self.out.push_str(",\n");
            }
            self.indent -= 1;
            self.margin();
        } else {
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    self.out.push_str(", ");
                }
                self.expression(item);
            }
        }
        self.out.push(']');
    }

    fn index(&mut self, base: &Expression, start: &Expression, limit: Option<&Expression>) {
        self.expression(base);
        self.out.push('[');
        self.expression(start);
        if let Some(end) = limit {
            self.out.push_str("..");
            self.expression(end);
        }
        self.out.push(']');
    }

    fn binary(&mut self, left: &Expression, operator: &str, right: &Expression, wrapped: bool) {
        self.expression(left);
        if operator == ".." {
            self.out.push_str("..");
            self.expression(right);
            return;
        }
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

    fn call(&mut self, expr: &Expression) {
        let Expression::Call {
            callee,
            generics,
            arguments,
            multiline,
            ..
        } = expr
        else {
            return;
        };
        self.expression(callee);
        if !generics.is_empty() {
            self.out.push('<');
            self.specs(generics);
            self.out.push('>');
        }
        self.out.push('(');
        if *multiline && !arguments.is_empty() {
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

    fn record(&mut self, expr: &Expression) {
        let Expression::Record {
            module,
            name,
            fields,
            multiline,
            ..
        } = expr
        else {
            return;
        };
        if let Some(prefix) = module {
            self.out.push_str(prefix);
            self.out.push('.');
        }
        self.out.push_str(name);
        if fields.is_empty() {
            self.out.push_str(" {}");
            return;
        }
        if *multiline {
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
