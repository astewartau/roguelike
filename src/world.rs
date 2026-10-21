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

use std::ops::Deref;

use hecs::{Bundle, Component, ComponentError, DynamicBundle, Entity, NoSuchEntity, Query, QueryMut, QueryOneError};

/// The game's world. See the module docs.
#[derive(Default)]
pub struct World {
    inner: hecs::World,
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
        self.inner.spawn(components)
    }

    /// Add (or replace) `components` on `entity`.
    pub fn insert(&mut self, entity: Entity, components: impl DynamicBundle) -> Result<(), NoSuchEntity> {
        self.inner.insert(entity, components)
    }

    /// Add (or replace) one component on `entity`.
    pub fn insert_one(&mut self, entity: Entity, component: impl Component) -> Result<(), NoSuchEntity> {
        self.inner.insert_one(entity, component)
    }

    /// Take `T` (a bundle of components) off `entity`. Fails, removing
    /// nothing, if any of them is missing.
    pub fn remove<T: Bundle + 'static>(&mut self, entity: Entity) -> Result<T, ComponentError> {
        self.inner.remove::<T>(entity)
    }

    /// Take one component off `entity`.
    pub fn remove_one<T: Component>(&mut self, entity: Entity) -> Result<T, ComponentError> {
        self.inner.remove_one::<T>(entity)
    }

    /// Delete `entity` and everything on it.
    pub fn despawn(&mut self, entity: Entity) -> Result<(), NoSuchEntity> {
        self.inner.despawn(entity)
    }

    /// Delete every entity.
    pub fn clear(&mut self) {
        self.inner.clear()
    }

    /// Iterate a query with unique access: changes component *values*, not
    /// which components entities have, so it needs no hooks.
    pub fn query_mut<Q: Query>(&mut self) -> QueryMut<'_, Q> {
        self.inner.query_mut::<Q>()
    }

    /// One entity's query result with unique access (values only, as above).
    pub fn query_one_mut<Q: Query>(&mut self, entity: Entity) -> Result<Q::Item<'_>, QueryOneError> {
        self.inner.query_one_mut::<Q>(entity)
    }
}
