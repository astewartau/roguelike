//! The game's ECS world: a hecs `World` behind a type of our own, so that
//! every change to what an entity is made of goes through one place.
//!
//! Reading is unchanged: `World` derefs to the hecs world, so queries,
//! `get` (including `get::<&mut T>`, which edits a component's values in
//! place), `contains` and `entity` all work as before. What it deliberately
//! does *not* hand out is mutable access to the hecs world itself, and hecs
//! needs that to spawn, insert, remove or despawn. So the compiler only
//! allows those through the methods below — the single point where the
//! game can react to an entity's makeup changing (component hooks, the way
//! Bevy's `on_add`/`on_remove` work), rather than every caller having to
//! remember to.
//!
//! The hooks it runs now: **requirements** (`component_registry::REQUIRES`).
//! Adding a component adds whatever it requires, at its default, if the
//! entity lacks it; removing a component also removes whatever requires it.
//! So no entity ever holds a component without what that component needs,
//! however it was built: spawn code, the developer tools, a floor reload.
//!
//! And a **journal** of the changes the engine's own bookkeeping has to
//! follow (`take_changes`): entities despawned (to leave the collision
//! cache, the scheduler and the AI tracker) and entities whose blocking
//! changed (for the collision cache). Code no longer has to remember to
//! tell each of them; the engine applies the journal as it goes
//! (`GameState::sync_world_changes`).

use std::any::TypeId;
use std::ops::Deref;

use hecs::{
    Component, ComponentError, DynamicBundle, Entity, NoSuchEntity, PreparedQuery, PreparedQueryIter, Query, QueryMut,
};

/// Declares cached queries, one per-thread `hecs::PreparedQuery` each, for
/// sweeps that run on every step of the simulation.
///
/// Most of a plain query's cost is matching it against every archetype in
/// the world, and a floor has dozens of those, most of them empty (hecs
/// never frees one, and adding or removing a component makes new ones). A
/// prepared query keeps its matching and redoes it only when the world
/// gains an archetype (or it is used on a different world), so a sweep
/// costs only the entities it visits. Iteration order is the same as a
/// plain query's.
///
/// ```ignore
/// prepared_query! { STATUS: &'static mut StatusEffects; }
/// STATUS.with_borrow_mut(|q| for (_, effects) in world.prepared_query_mut(q) { ... });
/// ```
macro_rules! prepared_query {
    ($($name:ident: $q:ty;)+) => {
        thread_local! {
            $(static $name: std::cell::RefCell<hecs::PreparedQuery<$q>> =
                std::cell::RefCell::new(hecs::PreparedQuery::new());)+
        }
    };
}
pub(crate) use prepared_query;

/// A change to an entity the engine's bookkeeping has to follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldChange {
    /// It's gone.
    Despawned(Entity),
    /// It gained or lost `BlocksMovement` or `BlocksVision` (or appeared
    /// with one).
    Blocking(Entity),
}

/// The game's world. See the module docs.
#[derive(Default)]
pub struct World {
    inner: hecs::World,
    changes: Vec<WorldChange>,
}

impl Deref for World {
    type Target = hecs::World;

    fn deref(&self) -> &hecs::World {
        &self.inner
    }
}

impl World {
    pub fn new() -> Self {
        Self::default()
    }

    /// Make a new entity from `components`.
    pub fn spawn(&mut self, components: impl DynamicBundle) -> Entity {
        let entity = self.inner.spawn(components);
        let added = self.types_of(entity);
        self.after_adding(entity, &added);
        entity
    }

    /// Add (or replace) `components` on `entity`.
    pub fn insert(&mut self, entity: Entity, components: impl DynamicBundle) -> Result<(), NoSuchEntity> {
        let before = self.types_of(entity);
        self.inner.insert(entity, components)?;
        let added: Vec<TypeId> = self.types_of(entity).into_iter().filter(|t| !before.contains(t)).collect();
        self.after_adding(entity, &added);
        Ok(())
    }

    /// Add (or replace) one component on `entity`.
    pub fn insert_one(&mut self, entity: Entity, component: impl Component) -> Result<(), NoSuchEntity> {
        self.insert(entity, (component,))
    }

    /// Take one component off `entity`. (There's deliberately no bundle
    /// `remove`: hecs's removes nothing at all if any one of the bundle is
    /// missing, which has caused bugs here before. Remove one at a time.)
    pub fn remove_one<T: Component>(&mut self, entity: Entity) -> Result<T, ComponentError> {
        let removed = self.inner.remove_one::<T>(entity)?;
        self.after_removing(entity, TypeId::of::<T>());
        Ok(removed)
    }

    /// Delete `entity` and everything on it, and every item it holds
    /// (`systems::held`): a held item has no existence apart from its
    /// holder. Move items out first to keep them.
    pub fn despawn(&mut self, entity: Entity) -> Result<(), NoSuchEntity> {
        let held = crate::systems::held::held_by_all(&self.inner, entity);
        self.inner.despawn(entity)?;
        self.changes.push(WorldChange::Despawned(entity));
        for item in held {
            if self.inner.despawn(item).is_ok() {
                self.changes.push(WorldChange::Despawned(item));
            }
        }
        Ok(())
    }

    /// The changes since the last call, for the engine to apply to its
    /// bookkeeping. See [`WorldChange`].
    pub fn take_changes(&mut self) -> Vec<WorldChange> {
        std::mem::take(&mut self.changes)
    }

    /// Iterate a query with unique access: changes component *values*, not
    /// which components entities have, so it needs no hooks.
    pub fn query_mut<Q: Query>(&mut self) -> QueryMut<'_, Q> {
        self.inner.query_mut::<Q>()
    }

    /// `query_mut` with the archetype matching done in advance: see
    /// [`prepared_query!`].
    pub fn prepared_query_mut<'q, Q: Query>(&'q mut self, query: &'q mut PreparedQuery<Q>) -> PreparedQueryIter<'q, Q> {
        query.query_mut(&mut self.inner)
    }

    /// The component types `entity` has (none if it's gone).
    fn types_of(&self, entity: Entity) -> Vec<TypeId> {
        self.inner.entity(entity).map(|e| e.component_types().collect()).unwrap_or_default()
    }

    fn has_type(&self, entity: Entity, t: TypeId) -> bool {
        self.inner.entity(entity).is_ok_and(|e| e.component_types().any(|c| c == t))
    }

    /// Hook: components of these types were just added to `entity`. Bring
    /// in what they require. (Adding one goes back through `insert`, so a
    /// requirement's own requirements follow.)
    fn after_adding(&mut self, entity: Entity, added: &[TypeId]) {
        if added.iter().any(|t| is_blocker(*t)) {
            self.changes.push(WorldChange::Blocking(entity));
        }
        for t in added {
            for needed in crate::component_registry::requires_of(*t) {
                if !self.has_type(entity, (needed.type_id)()) {
                    let _ = (needed.write)(self, entity, (needed.default)());
                }
            }
        }
    }

    /// Hook: a component of type `t` was just removed from `entity`. Take
    /// away what can't work without it. (Removing one goes back through
    /// `remove_one`, so it cascades.)
    fn after_removing(&mut self, entity: Entity, t: TypeId) {
        if is_blocker(t) {
            self.changes.push(WorldChange::Blocking(entity));
        }
        for dependent in crate::component_registry::required_by(t) {
            if self.has_type(entity, (dependent.type_id)()) {
                (dependent.remove)(self, entity);
            }
        }
    }
}

/// Whether components of type `t` decide what blocks a tile.
fn is_blocker(t: TypeId) -> bool {
    t == TypeId::of::<crate::components::BlocksMovement>() || t == TypeId::of::<crate::components::BlocksVision>()
}
