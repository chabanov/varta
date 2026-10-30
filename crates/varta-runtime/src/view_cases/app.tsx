import { NumberSignal, TextSignal, effect } from './signals.ts';

// A counter, a message box that sends on Enter (Shift+Enter inserts a
// newline, IME composition never sends) and a copy button.
function Counter(props: { start: number }): number {
  const count = new NumberSignal(props.start);
  return (
    <section aria-label="Counter">
      <p id="count">{count.get()}</p>
      <button id="inc" class="primary" onClick={() => count.set(count.get() + 1)}>
        +
      </button>
    </section>
  );
}

function Chat(): number {
  const draft = new TextSignal('');
  const sent = new TextSignal('');
  const status = new TextSignal('idle');
  return (
    <section aria-label="Chat">
      <label for="box">Message</label>
      <textarea
        id="box"
        rows="2"
        style="field-sizing: content; max-height: 120px"
        value={draft.get()}
        onInput={(event: ViewEvent) => draft.set(event.value)}
        onKeyDown:Enter-noShift-notComposing-preventDefault={() => {
          sent.set(draft.get());
          draft.set('');
        }}
      />
      <p id="sent">{sent.get()}</p>
      <button
        id="copy"
        type="button"
        disabled={sent.get() === ''}
        onClick={() =>
          View.copyText(sent.get(), (result: number) =>
            status.set(result === 0 ? 'copied' : `failed ${result}`),
          )
        }
      >
        Copy
      </button>
      <span id="status" aria-live="polite">{status.get()}</span>
    </section>
  );
}

export function main(): void {
  View.setEffect(effect);
  View.stylesheet(1, '.primary { color: #fff; background-color: #0050d0 }');
  View.mount(
    <main id="app">
      <h1>Varta</h1>
      <Counter start={1} />
      <Chat />
    </main>,
  );
}
