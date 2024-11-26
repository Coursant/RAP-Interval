use std::collections::HashMap;

struct ConstraintGraph {
    // The graph is represented as a map from a variable to a set of constraints
    // that involve that variable.
    constraints: HashMap<Variable, HashSet<Constraint>>,
}