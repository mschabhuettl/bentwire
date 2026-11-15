# bentwire-mqtt: MQTT bridge with Home Assistant discovery

`bentwire-mqtt` connects bentwired to an MQTT broker. Messages on a few topics make the
character speak, play an animation, show, hide or ask a question; the answer to a question
comes back on a topic. Home Assistant finds the bridge through MQTT discovery. Node-RED and
any other MQTT client can use the same topics. The rules are spec B-MQTT-1
([docs/spec.md](../../docs/spec.md)).

It runs in your desktop session, next to bentwired, and talks to it over the session bus.
Plain TCP, optionally with a user name and password; no TLS.

## Run

```sh
cargo build --release -p bentwire-mqtt
target/release/bentwire-mqtt --host broker.local
```

Options (they override the config file): `--host HOST`, `--port PORT`, `--base TOPIC`.

As a systemd user service: copy
[packaging/systemd/bentwire-mqtt.service](../../packaging/systemd/bentwire-mqtt.service) to
`~/.config/systemd/user/`, adjust `ExecStart`, then
`systemctl --user enable --now bentwire-mqtt`. It is not enabled by default.

## Configuration

`$XDG_CONFIG_HOME/bentwire/mqtt.toml` (usually `~/.config/bentwire/mqtt.toml`). Every key
is optional; a missing file means all defaults.

```toml
host = "localhost"            # broker
port = 1883
username = "bentwire"         # no credentials when absent
password = "secret"           # never logged
base_topic = "bentwire"
client_id = "bentwire-mqtt"   # must be unique on the broker
discovery = true              # Home Assistant discovery configs
discovery_prefix = "homeassistant"
```

The file holds a password: `chmod 600 ~/.config/bentwire/mqtt.toml`.

## Topics

`<base>` is `base_topic`.

| Topic | Direction | Payload |
|---|---|---|
| `<base>/say` | to the bridge | text to speak (`Speak`) |
| `<base>/play` | to the bridge | animation name (`Play`, see `bentwire animations`) |
| `<base>/show` | to the bridge | ignored (`Show`) |
| `<base>/hide` | to the bridge | ignored (`Hide`) |
| `<base>/ask` | to the bridge | plain text (one OK button), or JSON with the `Ask` keys (below) |
| `<base>/answer` | from the bridge | the answer: `button:<n>`, `ok`, `cancel`, `yes`, `no`, ..., `label:<n>` or `none` |
| `<base>/answer/checked` | from the bridge | `1,0,...` for asks with check boxes, sent just before the answer |
| `<base>/state` | from the bridge, retained | `visible` or `hidden` |
| `<base>/status` | from the bridge, retained | `online` or `offline` (also the MQTT last will) |

The JSON for `ask` takes the keys of the daemon's `Ask` call (spec B-ASK-1), each a string
or a list of strings: `text`, `heading`, `icon`, `buttons` (a set name such as `yes-no`, or
a list of up to 6 captions), `labels`, `label_type`, `checkboxes`, `mode`, `animation`.

```json
{"heading": "Coffee", "text": "Make some?", "buttons": ["Yes please", "Later"]}
```

The answer uses the names of `bentwire ask`: a custom button is `button:<n>` (1-based), a
set button its name (`ok`, `yes`, `no`, `cancel`, ...), a clicked label `label:<n>`. It is
`none` when the balloon was dismissed or closed (Hide, Stop, a new character), when the ask
failed (the character is hidden, the balloon is invalid) and when bentwired went away.

Retained messages on the command topics are ignored, so a stale retained `say` does not
speak again on every reconnect. A command while bentwired is not running starts it through
D-Bus activation, as the CLI does.

## Home Assistant

With `discovery = true` the bridge publishes retained configs under
`<discovery_prefix>/<component>/<id>/config` on every connect, all for one device
"bentwire":

| Entity | Component | Topic |
|---|---|---|
| Say | `notify` | `<base>/say` |
| Show | `button` | `<base>/show` |
| Hide | `button` | `<base>/hide` |
| Last answer | `sensor` | `<base>/answer` |
| Visible | `binary_sensor` | `<base>/state` |

All use `<base>/status` as availability. Ids are the base topic with `/` replaced by `_`
plus the entity (`bentwire_say`), so two machines need different base topics.

### Example automations

Say something when the washing machine is done:

```yaml
automation:
  - alias: "bentwire: laundry done"
    triggers:
      - trigger: state
        entity_id: binary_sensor.washing_machine_running
        to: "off"
    actions:
      - action: notify.send_message
        target:
          entity_id: notify.bentwire_say
        data:
          message: "The laundry is done."
```

Ask a question and act on the answer. Trigger on the MQTT topic, not on the sensor: two
equal answers in a row do not change the sensor's state.

```yaml
automation:
  - alias: "bentwire: ask about the lights"
    triggers:
      - trigger: sun
        event: sunset
    actions:
      - action: mqtt.publish
        data:
          topic: bentwire/ask
          payload: >-
            {"heading": "Sunset", "text": "Turn on the living room lights?",
             "buttons": ["Yes", "No"]}
  - alias: "bentwire: answer about the lights"
    triggers:
      - trigger: mqtt
        topic: bentwire/answer
        payload: "button:1"
    actions:
      - action: light.turn_on
        target:
          entity_id: light.living_room
```

Hide the character when you leave and show it when you are back:

```yaml
automation:
  - alias: "bentwire: follow presence"
    triggers:
      - trigger: state
        entity_id: person.me
    actions:
      - action: button.press
        target:
          entity_id: >-
            {{ 'button.bentwire_show' if trigger.to_state.state == 'home'
               else 'button.bentwire_hide' }}
```

The entity ids above assume the default base topic; Home Assistant shows the actual ones
under the "bentwire" device.

## Node-RED

Use an `mqtt out` node with topic `bentwire/say` (or `play`, `show`, `hide`, `ask`) and the
text as `msg.payload`; for `ask` a JSON object as payload works too (Node-RED sends it as
JSON). An `mqtt in` node on `bentwire/answer` delivers the answer as a string, ready for a
`switch` node on `button:1`, `button:2` and so on.

## Testing

`cargo test -p bentwire-mqtt` checks topic parsing, payloads and the discovery JSON without
a broker. `scripts/mqtt-check.sh` runs the end-to-end test with a private mosquitto, a
private session bus and headless bentwired.
