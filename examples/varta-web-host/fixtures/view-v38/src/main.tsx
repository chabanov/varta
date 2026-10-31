// A v38 view: a counter and a message box, with no reactive library: each
// handler sets the text it changes. Clicks, edits and keys arrive as view events.
const NO_NODE = 0;
let count = 0;
let countText = 0;
let sentText = 0;
let draft = "";

function setCount(next: number): void {
  count = next;
  View.setText(countText, "count " + String(count));
}

function Counter(): number {
  countText = View.node();
  View.createText(countText, "count 0");
  const p = <p id="count" />;
  View.insert(p, countText, NO_NODE);
  return p;
}

function Sent(): number {
  sentText = View.node();
  View.createText(sentText, "nothing sent");
  const p = <p id="sent" aria-live="polite" />;
  View.insert(p, sentText, NO_NODE);
  return p;
}

export function main(): void {
  View.stylesheet(1, ".primary { color: #fff; background-color: #0050d0 }");
  View.mount(
    <main id="app">
      <h1>Varta</h1>
      <Counter />
      <button id="inc" class="primary" type="button" onClick={() => setCount(count + 1)}>
        +
      </button>
      <textarea
        id="box"
        rows="2"
        aria-label="Message"
        onInput={(event: ViewEvent) => {
          draft = event.value;
        }}
        onKeyDown:Enter-noShift-notComposing-preventDefault={() => View.setText(sentText, "sent " + draft)}
      />
      <Sent />
    </main>,
  );
}
