// Integrity levels and class construction, accessors and symbol-keyed
// members: fields a derived class defines after its base constructor froze
// the instance (§7.3.33 DefineField), setters of frozen objects, and the
// `Symbol(description)` keys of V8's messages.
function attempt(label: string, action: () => void): void {
  try {
    action();
    console.log(label + ': ok');
  } catch (error) {
    console.log(label + ': ' + String(error));
  }
}

const TAG = Symbol('tag');

interface Labelled {
  [TAG]?: string;
}

class Frozen {
  readonly id: number;
  constructor(id: number) {
    this.id = id;
    Object.freeze(this);
  }
}

class WithField extends Frozen {
  note = 'n';
}

class WithoutField extends Frozen {
  describe(): string {
    return 'id ' + String(this.id);
  }
}

class Sealing {
  count = 0;
  constructor() {
    Object.seal(this);
  }
  increment(): number {
    this.count++;
    return this.count;
  }
}

class Temperature {
  #celsius = 0;
  get fahrenheit(): number {
    return this.#celsius * 1.8 + 32;
  }
  set fahrenheit(value: number) {
    this.#celsius = (value - 32) / 1.8;
  }
}

export function main(): void {
  attempt('derived field', () => {
    new WithField(1);
  });
  attempt('derived without field', () => {
    console.log(new WithoutField(2).describe());
  });
  const sealing = new Sealing();
  console.log(String(sealing.increment()) + ' ' + String(Object.isSealed(sealing)));

  const temperature = Object.freeze(new Temperature());
  temperature.fahrenheit = 212;
  console.log(String(temperature.fahrenheit) + ' ' + String(Object.isFrozen(temperature)));

  let stored = 1;
  const accessor = {
    get value(): number {
      return stored;
    },
    set value(next: number) {
      stored = next;
    },
  };
  Object.seal(accessor);
  accessor.value = 5;
  console.log(String(accessor.value) + ' ' + String(Object.isFrozen(accessor)));

  const tagged = { [TAG]: 'a', plain: 1 };
  Object.freeze(tagged);
  attempt('symbol key', () => {
    tagged[TAG] = 'b';
  });
  attempt('string key', () => {
    tagged.plain = 2;
  });
  const labelled: Labelled = {};
  Object.preventExtensions(labelled);
  attempt('symbol add', () => {
    labelled[TAG] = 'c';
  });
  const sealedTag: Labelled = { [TAG]: 'd' };
  Object.seal(sealedTag);
  attempt('symbol delete', () => {
    delete sealedTag[TAG];
  });
  console.log(String(sealedTag[TAG]) + ' ' + String(labelled[TAG]));
}
