#!/usr/bin/env node
// bentwire D-Bus example in JavaScript (Node.js) with dbus-next (`npm install dbus-next`),
// docs/dbus.md. Speaks, plays an animation, asks a question with two buttons and prints
// the answer, then prints Click, DblClick, Shown and Hidden until Ctrl+C.
'use strict';

const dbus = require('dbus-next');

const NAME = 'org.bentwire.Agent';
const PATH = '/org/bentwire/Character';
const IFACE = 'org.bentwire.Character1';

function resultName(result) {
  if (result === 0) return 'none (dismissed or interrupted)';
  if (result >= 1) return `label ${result}`;
  if (result <= -25) return `button ${-24 - result}`;
  return `standard button ${result}`; // -1 OK, -2 Cancel, -3 Yes, ... (B-ASK-2)
}

async function main() {
  const bus = dbus.sessionBus();
  // Introspects the object; this D-Bus-activates bentwired if it is not running.
  const obj = await bus.getProxyObject(NAME, PATH);
  const ch = obj.getInterface(IFACE);

  // Subscribe before the first call so that no signal is missed. RequestComplete and
  // AskComplete are unicast to this connection (B-Q-2); the others are broadcast.
  const pending = new Map(); // request id -> callback(status, reason)
  const early = new Map(); // request id -> [status, reason], completed before `await` resumed
  const answers = new Map(); // ask id -> [result, checked]
  ch.on('AskComplete', (id, result, checked) => answers.set(id, [result, checked]));
  ch.on('RequestComplete', (id, status, reason) => {
    const done = pending.get(id);
    if (done) {
      pending.delete(id);
      done(status, reason);
    } else {
      early.set(id, [status, reason]);
    }
  });
  // The reply and RequestComplete can arrive together, and dbus-next may then run the
  // signal handler before the awaited call returns: look in `early` first.
  const whenComplete = (id, done) => {
    if (early.has(id)) {
      const [status, reason] = early.get(id);
      early.delete(id);
      done(status, reason);
    } else {
      pending.set(id, done);
    }
  };
  for (const member of ['Click', 'DblClick', 'Shown', 'Hidden']) {
    ch.on(member, (...args) => console.log(member, ...args));
  }

  await ch.Show(false);
  const spoken = await ch.Speak('Hello from JavaScript!');
  whenComplete(spoken, (status, reason) => console.log('spoken:', status, reason || 'ok'));
  await ch.Play('Wave');
  const ask = await ch.Ask({
    heading: new dbus.Variant('s', 'JavaScript example'),
    text: new dbus.Variant('s', 'Coffee or tea?'),
    buttons: new dbus.Variant('as', ['&Coffee', '&Tea']),
  });
  whenComplete(ask, (status, reason) => {
    if (status !== 0) console.log(`ask ended with status ${status} ${reason}`);
    const [result] = answers.get(ask) || [0, []];
    answers.delete(ask);
    console.log('answer:', resultName(result));
    console.log('Listening for Click, DblClick, Shown, Hidden (Ctrl+C to quit)');
  });

  for (const sig of ['SIGINT', 'SIGTERM']) {
    process.on(sig, () => {
      bus.disconnect();
      process.exit(0);
    });
  }
}

main().catch((e) => {
  // A method error has the D-Bus error name in e.type, e.g. org.bentwire.Error.NoCharacter.
  console.error(`bentwire: ${e.type || ''} ${e.text || e.message}`);
  process.exit(1);
});
