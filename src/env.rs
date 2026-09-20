//! Lexical scopes: a chain of maps ending at the global scope.
//!
//! A function value captures the scope it was defined in, which is what makes
//! closures work. Lookup walks outward; **assignment always binds in the
//! innermost scope**, since minipy has no `global`/`nonlocal`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::value::Value;

#[derive(Debug, Default)]
struct Scope {
    vars: HashMap<String, Value>,
    parent: Option<Env>,
}

#[derive(Debug, Clone)]
pub struct Env(Rc<RefCell<Scope>>);

impl Env {
    pub fn global() -> Env {
        Env(Rc::new(RefCell::new(Scope::default())))
    }

    /// A new innermost scope nested in this one.
    pub fn child(&self) -> Env {
        Env(Rc::new(RefCell::new(Scope {
            vars: HashMap::new(),
            parent: Some(self.clone()),
        })))
    }

    /// Look `name` up here, then outward through the enclosing scopes.
    pub fn get(&self, name: &str) -> Option<Value> {
        let scope = self.0.borrow();
        match scope.vars.get(name) {
            Some(value) => Some(value.clone()),
            None => scope.parent.as_ref()?.get(name),
        }
    }

    /// Bind `name` in this scope, shadowing any outer binding.
    pub fn set(&self, name: &str, value: Value) {
        self.0.borrow_mut().vars.insert(name.to_string(), value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_walks_outward() {
        let global = Env::global();
        global.set("x", Value::Int(1));
        let inner = global.child();
        assert_eq!(inner.get("x"), Some(Value::Int(1)));
        assert_eq!(inner.get("nope"), None);
    }

    #[test]
    fn assignment_shadows_rather_than_rebinding_the_outer_scope() {
        let global = Env::global();
        global.set("x", Value::Int(1));
        let inner = global.child();
        inner.set("x", Value::Int(2));
        assert_eq!(inner.get("x"), Some(Value::Int(2)));
        assert_eq!(global.get("x"), Some(Value::Int(1)));
    }
}
