//! Core game initialization and state management.

use hecs::{Entity, World};

/// Advance a dialogue by selecting an option.
/// Returns true if the dialogue ended (caller should close the UI).
/// Returns false if the dialogue continues.
pub fn advance_dialogue(world: &mut World, npc_entity: Entity, option_index: usize) -> bool {
    use crate::components::Dialogue;
    use crate::systems::dialogue as dialogue_sys;

    let Ok(mut dialogue) = world.get::<&mut Dialogue>(npc_entity) else {
        return true;
    };

    let continues = dialogue_sys::select_option(&mut dialogue, option_index);
    if !continues {
        dialogue_sys::reset_dialogue(&mut dialogue);
        return true;
    }

    false
}

/// Result of handling Enter key for containers
#[allow(dead_code)] // Entity fields reserved for caller to identify container
pub enum ContainerAction {
    /// No action taken
    None,
    /// Took all from the specified chest (caller should close chest UI)
    TookAll(Entity),
    /// Opened a container at player position (caller should process events)
    Opened(Entity),
}

/// Handle Enter key for container interaction.
pub fn handle_enter_key_container(
    world: &mut World,
    player_entity: Entity,
    open_chest: Option<Entity>,
    events: &mut crate::events::EventQueue,
) -> ContainerAction {
    if let Some(chest_id) = open_chest {
        crate::systems::take_all_from_container(world, player_entity, chest_id, Some(events));
        return ContainerAction::TookAll(chest_id);
    }

    if let Some(container_id) = crate::systems::find_container_at_player(world, player_entity) {
        events.push(crate::events::GameEvent::ContainerOpened {
            container: container_id,
            opener: player_entity,
        });
        return ContainerAction::Opened(container_id);
    }

    ContainerAction::None
}
