// A large pretty-print expansion built in bounded turns under normal budgets.
let parent = 1;
let branch = 0;
let depth = 0;
let leaves = 0;

function build(): void {
  for (let i = 0; i < 32; i++) {
    if (depth < 500) {
      const node = <span />;
      View.insert(parent, node, 0);
      if (depth === 0) branch = node;
      parent = node;
      depth++;
    } else if (leaves < 3000) {
      const text = View.node();
      View.createText(text, "x");
      View.insert(parent, text, 0);
      leaves++;
    }
  }
  View.commit();
  if (leaves < 3000) setTimeout(build, 0);
  else console.log("READY");
}

export function main(): void {
  const clear = <button id="clear" onClick={(event: ViewEvent): void => {
    View.remove(branch);
    View.setTitle("recovered");
    View.commit();
  }}>Clear</button>;
  View.insert(1, clear, 0);
  build();
}
