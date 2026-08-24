//! There is no actual game, it will just display the current
//! settings for 5 seconds before going back to the menu.

use bevy::prelude::*;

const TEXT_COLOR: Color = Color::srgb(0.9, 0.9, 0.9);

use bevy::{
    color::palettes::basic::{BLUE, LIME},
};

use crate::{GameState, Volume};

// This plugin will contain the game. In this case, it's just be a screen that will
// display the current settings for 5 seconds before returning to the menu
pub fn game_plugin(app: &mut App) {
    app.add_systems(OnEnter(GameState::Game), game_setup)
        .add_systems(Update, game.run_if(in_state(GameState::Game)));
}

// Tag component used to tag entities added on the game screen
#[derive(Component)]
struct OnGameScreen;

#[derive(Resource, Deref, DerefMut)]
struct GameTimer(Timer);

fn game_setup(
    mut commands: Commands,
    display_quality: Res<crate::DisplayQuality>,
    volume: Res<crate::Volume>,
) {
    commands.spawn((
        DespawnOnExit(GameState::Game),
        Node {
            width: percent(100),
            height: percent(100),
            // center children
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        OnGameScreen,
        children![(
            Node {
                // This will display its children in a column, from top to bottom
                flex_direction: FlexDirection::Column,
                // `align_items` will align children on the cross axis. Here the main axis is
                // vertical (column), so the cross axis is horizontal. This will center the
                // children
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::BLACK),
            children![
                (
                    Text::new("Will be back to the menu shortly..."),
                    TextFont {
                        font_size: FontSize::Px(67.0),
                        ..default()
                    },
                    TextColor(TEXT_COLOR),
                    Node {
                        margin: UiRect::all(px(50)),
                        ..default()
                    },
                ),
                (
                    Text::default(),
                    Node {
                        margin: UiRect::all(px(50)),
                        ..default()
                    },
                    children![
                        (
                            TextSpan(format!("quality: {:?}", *display_quality)),
                            TextFont {
                                font_size: FontSize::Px(50.0),
                                ..default()
                            },
                            TextColor(BLUE.into()),
                        ),
                        (
                            TextSpan::new(" - "),
                            TextFont {
                                font_size: FontSize::Px(50.0),
                                ..default()
                            },
                            TextColor(TEXT_COLOR),
                        ),
                        (
                            TextSpan(format!("volume: {:?}", *volume)),
                            TextFont {
                                font_size: FontSize::Px(50.0),
                                ..default()
                            },
                            TextColor(LIME.into()),
                        ),
                    ]
                ),
            ]
        )],
    ));
    // Spawn a 5 seconds timer to trigger going back to the menu
    commands.insert_resource(GameTimer(Timer::from_seconds(5.0, TimerMode::Once)));
}

// Tick the timer, and change state when finished
fn game(
    time: Res<Time>,
    mut game_state: ResMut<NextState<GameState>>,
    mut timer: ResMut<GameTimer>,
) {
    if timer.tick(time.delta()).is_finished() {
        game_state.set(GameState::Menu);
    }
}
