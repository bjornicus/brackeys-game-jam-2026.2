use bevy::prelude::*;
use map_support::dialogue::DialogueLine;

use crate::GameState;

const PANEL_COLOR: Color = Color::srgba(0.04, 0.07, 0.12, 0.96);
const BACKDROP_COLOR: Color = Color::srgba(0.0, 0.0, 0.0, 0.62);

/// Identifies why the currently displayed modal was opened.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DialogueSource {
    Terminal,
    Unlock,
}

/// Owned dialogue state. It never borrows from an asset which may be reloaded.
#[derive(Resource, Clone, Debug, Eq, PartialEq)]
pub(crate) struct ActiveDialogue {
    pub(crate) lines: Vec<DialogueLine>,
    pub(crate) line_index: usize,
    pub(crate) source: DialogueSource,
}

impl ActiveDialogue {
    fn current_line(&self) -> &DialogueLine {
        &self.lines[self.line_index]
    }

    fn is_final_line(&self) -> bool {
        self.line_index + 1 == self.lines.len()
    }
}

#[derive(Component)]
struct DialoguePanel;

#[derive(Component)]
struct DialogueSpeakerText;

#[derive(Component)]
struct DialogueBodyText;

/// Installs a non-empty owned conversation and schedules the dialogue modal.
///
/// Returns `false` for invalid empty input and deliberately leaves the current state unchanged.
pub(crate) fn request_dialogue(
    commands: &mut Commands,
    next_state: &mut NextState<GameState>,
    lines: Vec<DialogueLine>,
    source: DialogueSource,
) -> bool {
    if lines.is_empty() {
        warn!("ignored empty dialogue request");
        return false;
    }
    commands.insert_resource(ActiveDialogue {
        lines,
        line_index: 0,
        source,
    });
    next_state.set(GameState::Dialogue);
    true
}

pub(crate) fn game_dialogue_plugin(app: &mut App) {
    app.add_systems(OnEnter(GameState::Dialogue), setup_dialogue_ui)
        .add_systems(
            Update,
            advance_dialogue_input.run_if(in_state(GameState::Dialogue)),
        );
}

fn setup_dialogue_ui(mut commands: Commands, dialogue: Res<ActiveDialogue>) {
    let line = dialogue.current_line();
    commands.spawn((
        DespawnOnExit(GameState::Dialogue),
        DialoguePanel,
        Node {
            width: percent(100),
            height: percent(100),
            position_type: PositionType::Absolute,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(BACKDROP_COLOR),
        children![(
            Node {
                width: percent(78),
                max_width: px(900),
                min_height: px(190),
                padding: UiRect::all(px(28)),
                flex_direction: FlexDirection::Column,
                row_gap: px(12),
                ..default()
            },
            BackgroundColor(PANEL_COLOR),
            children![
                (
                    DialogueSpeakerText,
                    Text::new(line.speaker.display_label()),
                    TextFont {
                        font_size: FontSize::Px(24.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.45, 0.8, 1.0)),
                ),
                (
                    DialogueBodyText,
                    Text::new(&line.text),
                    TextFont {
                        font_size: FontSize::Px(28.0),
                        ..default()
                    },
                    TextColor(Color::WHITE),
                    Node {
                        width: percent(100),
                        ..default()
                    },
                ),
                (
                    Text::new("Space / Enter / Click"),
                    TextFont {
                        font_size: FontSize::Px(16.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.7, 0.7, 0.75)),
                ),
            ],
        )],
    ));
}

fn advance_dialogue_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut commands: Commands,
    dialogue: Option<ResMut<ActiveDialogue>>,
    mut next_state: ResMut<NextState<GameState>>,
    mut speaker_text: Query<&mut Text, With<DialogueSpeakerText>>,
    mut body_text: Query<&mut Text, (With<DialogueBodyText>, Without<DialogueSpeakerText>)>,
) {
    let advance_pressed = keyboard.just_pressed(KeyCode::Space)
        || keyboard.just_pressed(KeyCode::Enter)
        || keyboard.just_pressed(KeyCode::NumpadEnter)
        || mouse.just_pressed(MouseButton::Left);
    if !advance_pressed {
        return;
    }
    advance_dialogue(
        &mut commands,
        dialogue,
        &mut next_state,
        &mut speaker_text,
        &mut body_text,
    );
}

fn advance_dialogue(
    commands: &mut Commands,
    dialogue: Option<ResMut<ActiveDialogue>>,
    next_state: &mut NextState<GameState>,
    speaker_text: &mut Query<&mut Text, With<DialogueSpeakerText>>,
    body_text: &mut Query<&mut Text, (With<DialogueBodyText>, Without<DialogueSpeakerText>)>,
) {
    let Some(mut dialogue) = dialogue else {
        return;
    };
    if dialogue.is_final_line() {
        commands.remove_resource::<ActiveDialogue>();
        next_state.set(GameState::Game);
        return;
    }

    dialogue.line_index += 1;
    let line = dialogue.current_line();
    for mut text in speaker_text.iter_mut() {
        text.0 = line.speaker.display_label().to_owned();
    }
    for mut text in body_text.iter_mut() {
        text.0 = line.text.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::state::app::StatesPlugin;
    use map_support::dialogue::Speaker;

    #[derive(Debug, Default, PartialEq, Resource)]
    struct GameplayTimerProbe(u32);

    fn tick_probe(mut probe: ResMut<GameplayTimerProbe>) {
        probe.0 += 1;
    }

    fn line(speaker: Speaker, text: &str) -> DialogueLine {
        DialogueLine {
            speaker,
            text: text.to_owned(),
        }
    }

    #[test]
    fn requests_keep_order_and_reject_empty_lines() {
        let mut world = World::new();
        world.init_resource::<NextState<GameState>>();
        let mut next = world.remove_resource::<NextState<GameState>>().unwrap();
        let mut commands = world.commands();
        assert!(!request_dialogue(
            &mut commands,
            &mut next,
            vec![],
            DialogueSource::Terminal,
        ));
        drop(commands);
        world.insert_resource(next);
        world.flush();
        assert!(!world.contains_resource::<ActiveDialogue>());

        let mut next = world.remove_resource::<NextState<GameState>>().unwrap();
        let mut commands = world.commands();
        assert!(request_dialogue(
            &mut commands,
            &mut next,
            vec![
                line(Speaker::NoOne, "first"),
                line(Speaker::NoFive, "second")
            ],
            DialogueSource::Terminal,
        ));
        drop(commands);
        world.insert_resource(next);
        world.flush();
        let active = world.resource::<ActiveDialogue>();
        assert_eq!(active.current_line().text, "first");
        assert_eq!(active.lines[1].text, "second");
    }

    #[test]
    fn dialogue_state_freezes_gameplay_probe_and_escape_is_ignored() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .init_state::<GameState>()
            .init_resource::<GameplayTimerProbe>()
            .insert_resource(ButtonInput::<KeyCode>::default())
            .insert_resource(ButtonInput::<MouseButton>::default())
            .add_systems(Update, tick_probe.run_if(in_state(GameState::Game)))
            .add_systems(
                Update,
                advance_dialogue_input.run_if(in_state(GameState::Dialogue)),
            );
        app.world_mut().insert_resource(ActiveDialogue {
            lines: vec![line(Speaker::System, "hold")],
            line_index: 0,
            source: DialogueSource::Terminal,
        });
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Dialogue);
        app.update();
        assert_eq!(
            *app.world().resource::<GameplayTimerProbe>(),
            GameplayTimerProbe(0)
        );
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        assert_eq!(
            app.world().resource::<State<GameState>>().get(),
            &GameState::Dialogue
        );
        assert!(app.world().contains_resource::<ActiveDialogue>());
    }

    #[test]
    fn modal_ui_uses_display_labels_and_is_cleaned_up_on_close() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .init_state::<GameState>()
            .insert_resource(ButtonInput::<KeyCode>::default())
            .insert_resource(ButtonInput::<MouseButton>::default())
            .add_plugins(game_dialogue_plugin);
        app.world_mut().insert_resource(ActiveDialogue {
            lines: vec![line(Speaker::NoFive, "visible")],
            line_index: 0,
            source: DialogueSource::Terminal,
        });
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Dialogue);
        app.update();
        {
            let world = app.world_mut();
            assert_eq!(world.query::<&DialoguePanel>().iter(world).count(), 1);
            assert!(
                world
                    .query_filtered::<&Text, With<DialogueSpeakerText>>()
                    .iter(world)
                    .any(|text| text.0 == "no. five")
            );
        }

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);
        app.update();
        app.update();
        assert_eq!(
            app.world().resource::<State<GameState>>().get(),
            &GameState::Game
        );
        let world = app.world_mut();
        assert_eq!(world.query::<&DialoguePanel>().iter(world).count(), 0);
    }

    #[test]
    fn one_advance_moves_one_line_and_final_advance_closes() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin))
            .init_state::<GameState>()
            .insert_resource(ButtonInput::<KeyCode>::default())
            .insert_resource(ButtonInput::<MouseButton>::default())
            .add_systems(
                Update,
                advance_dialogue_input.run_if(in_state(GameState::Dialogue)),
            );
        app.world_mut().insert_resource(ActiveDialogue {
            lines: vec![line(Speaker::NoOne, "one"), line(Speaker::NoTwo, "two")],
            line_index: 0,
            source: DialogueSource::Unlock,
        });
        app.world_mut()
            .resource_mut::<NextState<GameState>>()
            .set(GameState::Dialogue);
        app.update();

        {
            let mut keyboard = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keyboard.press(KeyCode::Space);
            keyboard.press(KeyCode::Enter);
        }
        app.update();
        assert_eq!(app.world().resource::<ActiveDialogue>().line_index, 1);
        assert_eq!(
            app.world().resource::<State<GameState>>().get(),
            &GameState::Dialogue
        );

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        assert!(!app.world().contains_resource::<ActiveDialogue>());
        app.update();
        assert_eq!(
            app.world().resource::<State<GameState>>().get(),
            &GameState::Game
        );
    }
}
