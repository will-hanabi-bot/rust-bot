# rust-bot

A deterministic Rust bot that plays on the [hanab.live](https://hanab.live/) interface. Basic structure and ideas were taken from [Zamiell's example bot](https://github.com/Zamiell/hanabi-live-bot) (Python). Fork of my [more-developed bot](https://github.com/will-hanabi-bot/hanabi-bot) (JavaScript).

It only plays with [Reactor 1.0 conventions](https://hanabi.wiki/en/conventions/reactor). Just like my other bot, it does not do any "learning" during the game.

## Bot features

- It can play No Variant.
- Takes notes during the game on what it thinks each player knows about their own hand.
- Can replay completed games on hanab.live and offer suggested actions.

This was mainly an experiment to see if Rust was a good language for me (it isn't), so feature parity with my other bot is very unlikely.

## Running locally

- You'll need to have Rust and cargo (Rust's package manager) installed. There are instructions [here](https://www.rust-lang.org/tools/install).
- Clone the repository to your own computer. There are lots of tutorials online on using Git if you don't know how that works.
- Navigate to the cloned repository in a terminal.
- Fill out the login details for the bot in an .env file. See .env.template for an example.
  - You'll need to create its account on hanab.live first.
- Run `cargo run --bin main -- index=<index>` to start the bot.
- Debug logs will show up in the console, providing more information about what the bot thinks about every action.
  - `hand <playerName>` will display the information on that player's hand from the common knowledge perspective.

## Supported commands

Send a PM to the bot on hanab.live (`/pm <HANABI_USERNAME> <message>`) to interact with it.
- `/join [password]` to join your current lobby. The bot will remain in your table until it is kicked with `/leave`.
- `/rejoin` to rejoin a game that has already started (e.g. if it crashed).
- `/leave` to kick the bot from your table.
- `/version` to get the current version of the bot.

Some commands can be sent inside a room to affect all bots that have joined.
- `/leaveall` to kick all bots from the table.

## Watching replays

A replay from hanab.live or from a file (in JSON) can be simulated using `cargo run --bin replay -- <options>`.
- `id=<id>` indicates the ID of the hanab.live replay to load
- `index=<index>` sets the index of the player the bot will simulate as (defaults to 0)

In a replay, the following commands are also supported (in addition to `hand`):
- `navigate <turn>` to travel to a specific turn.
    - If it is the bot's turn, it will provide a suggestion on what it would do.
