//! Lexical scopes: a chain of maps ending at the global scope.
//!
//! A function value captures the scope it was defined in, which is what makes
//! closures work. Lookup walks outward; **assignment always binds in the
//! innermost scope**, since minipy has no `global`/`nonlocal`.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::value::Value;

#[derive(Debug, Default)]
struct Scope {
    vars: HashMap<String, Value>,
    parent: Option<Env>,
    /// Names the enclosing function binds somewhere in its body. Reading one
    /// before it is bound is an `UnboundLocalError`, not a look outward.
    declared: Rc<HashSet<String>>,
}

/// The outcome of resolving a name.
#[derive(Debug)]
pub enum Lookup {
    Found(Value),
    /// Local to this scope, but not bound yet.
    Unbound,
    /// Local to an enclosing function, but not bound yet.
    UnboundFree,
    /// Not known anywhere.
    Missing,
}

#[derive(Debug, Clone)]
pub struct Env(Rc<RefCell<Scope>>);

impl Env {
    pub fn global() -> Env {
        Env(Rc::new(RefCell::new(Scope::default())))
    }

    /// A new innermost scope nested in this one.
    pub fn child(&self) -> Env {
        self.frame(Rc::new(HashSet::new()))
    }

    /// A call frame nested in this scope, knowing which names are local to it.
    pub fn frame(&self, declared: Rc<HashSet<String>>) -> Env {
        Env(Rc::new(RefCell::new(Scope {
            vars: HashMap::new(),
            parent: Some(self.clone()),
            declared,
        })))
    }

    /// Whether this scope's function binds `name` somewhere in its body.
    pub fn declares(&self, name: &str) -> bool {
        self.0.borrow().declared.contains(name)
    }

    /// Look `name` up here, then outward through the enclosing scopes.
    pub fn get(&self, name: &str) -> Option<Value> {
        match self.lookup(name) {
            Lookup::Found(value) => Some(value),
            _ => None,
        }
    }

    /// Resolve `name`, reporting *why* it is missing when it is.
    ///
    /// A function scope that declares a name but has not bound it yet stops
    /// the search: Python does not fall through to an outer binding, it
    /// reports that the local (or free) variable has no value.
    pub fn lookup(&self, name: &str) -> Lookup {
        let scope = self.0.borrow();
        if let Some(value) = scope.vars.get(name) {
            return Lookup::Found(value.clone());
        }
        if scope.declared.contains(name) {
            return Lookup::Unbound;
        }
        match scope.parent.as_ref() {
            None => Lookup::Missing,
            Some(parent) => match parent.lookup(name) {
                // An unbound name one scope out is a free variable here.
                Lookup::Unbound | Lookup::UnboundFree => Lookup::UnboundFree,
                found => found,
            },
        }
    }

    /// Look `name` up in *this* scope only.
    ///
    /// Augmented assignment needs this: `n += 1` makes `n` local to the
    /// function, so an outer `n` is not what it reads.
    pub fn get_local(&self, name: &str) -> Option<Value> {
        self.0.borrow().vars.get(name).cloned()
    }

    /// True for the module scope, which has nothing outside it.
    pub fn is_global(&self) -> bool {
        self.0.borrow().parent.is_none()
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
    fn local_lookup_does_not_walk_outward() {
        let global = Env::global();
        global.set("x", Value::Int(1));
        let inner = global.child();
        assert_eq!(inner.get("x"), Some(Value::Int(1)));
        assert_eq!(inner.get_local("x"), None);
        assert!(global.is_global());
        assert!(!inner.is_global());
    }

    #[test]
    fn a_declared_but_unbound_name_stops_the_search() {
        let global = Env::global();
        global.set("x", Value::Int(1));

        let declares_x = Rc::new(HashSet::from(["x".to_string()]));
        let frame = global.frame(declares_x);

        // The outer `x` is not what this frame reads: `x` is local here.
        assert!(matches!(frame.lookup("x"), Lookup::Unbound));
        assert_eq!(frame.get("x"), None);

        // One scope further in, the same name is a free variable.
        let inner = frame.child();
        assert!(matches!(inner.lookup("x"), Lookup::UnboundFree));

        // Once bound, it resolves normally.
        frame.set("x", Value::Int(2));
        assert!(matches!(frame.lookup("x"), Lookup::Found(Value::Int(2))));
        assert!(matches!(inner.lookup("x"), Lookup::Found(Value::Int(2))));
    }

    #[test]
    fn an_unknown_name_is_missing_not_unbound() {
        let global = Env::global();
        assert!(matches!(global.lookup("nope"), Lookup::Missing));
        assert!(matches!(global.child().lookup("nope"), Lookup::Missing));
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
