import { Runtime, effect, signal } from './reactive.ts';
import css from './styles.css' with { type: 'text' };

interface CardProps { rt: Runtime; }
interface Sample { name: string; values: number[]; }

function Header(): number {
  return (
    <div class="header">
      <div>
        <div class="brand">Varta Showcase</div>
        <div class="subtitle">TSX → WebAssembly → нативне вікно</div>
      </div>
      <div class="badge">ЖИВА ДЕМОНСТРАЦІЯ</div>
    </div>
  );
}

function CounterCard(props: CardProps): number {
  const count = signal<number>(props.rt, 0);
  const step = signal<number>(props.rt, 1);
  const highlighted = signal<boolean>(props.rt, false);
  return (
    <div class={highlighted.get() ? 'card selected' : 'card'}>
      <div class="heading">{'01 / Стан і події'}</div>
      <div class="muted">Кнопки змінюють стан усередині Wasm.</div>
      <div class="counter">{count.get()}</div>
      <div class="row">
        <button id="decrement" type="button" class="button" aria-label="Зменшити"
          onClick={() => count.set(count.get() - step.get())}>−</button>
        <button id="increment" type="button" class="button primary" aria-label="Збільшити"
          onClick={() => count.set(count.get() + step.get())}>+</button>
        <button id="reset" type="button" class="button" onClick={() => count.set(0)}>Скинути</button>
      </div>
      <div class="row">
        <input id="step-one" type="radio" name="step" class="check" aria-label="Крок один"
          checked={step.get() === 1}
          onInput={(event: ViewEvent) => { if (event.checked()) step.set(1); }} />
        <div>Крок 1</div>
        <input id="step-five" type="radio" name="step" class="check" aria-label="Крок п’ять"
          checked={step.get() === 5}
          onInput={(event: ViewEvent) => { if (event.checked()) step.set(5); }} />
        <div>Крок 5</div>
      </div>
      <div class="row">
        <input id="highlight" type="checkbox" class="check" aria-label="Підсвітити картку"
          checked={highlighted.get()}
          onInput={(event: ViewEvent) => highlighted.set(event.checked())} />
        <div>Підсвітити картку</div>
      </div>
    </div>
  );
}

function EditorCard(props: CardProps): number {
  const name = signal<string>(props.rt, '');
  const note = signal<string>(props.rt, 'Український текст працює.\nМожна виділяти, редагувати та скасовувати зміни.');
  const edited = signal<boolean>(props.rt, false);
  return (
    <div class="card">
      <div class="heading">{'02 / Редагування'}</div>
      <input id="name" type="text" class="field" placeholder="Як вас звати?" aria-label="Ваше ім’я"
        value={name.get()} onInput={(event: ViewEvent) => name.set(event.value)} />
      <div class="muted">{name.get() === '' ? 'Вітаю! Спробуйте ввести своє ім’я.' : `Вітаю, ${name.get()}!`}</div>
      <textarea id="editor" class="field editor" aria-label="Нотатка" value={note.get()}
        onInput={(event: ViewEvent) => { note.set(event.value); edited.set(true); }} />
      <div class="muted">{edited.get() ? `Довжина нотатки: ${note.get().length} символів` : 'Редактор: виділення, переноси, Undo / Redo.'}</div>
    </div>
  );
}

function TypographyCard(props: CardProps): number {
  const topic = signal<string>(props.rt, 'ui');
  return (
    <div class="card">
      <div class="heading">{'03 / Текст і компонування'}</div>
      <div class="inline">Це спільний рядок із <span class="accent">кольоровим фрагментом</span>, успадкуванням стилів та природними переносами між вузлами.</div>
      <select id="topic" class="field" aria-label="Тема демонстрації" value={topic.get()}
        onInput={(event: ViewEvent) => topic.set(event.value)}>
        <option value="ui">Інтерфейс</option>
        <option value="core">Ядро</option>
        <option value="all">Усе разом</option>
      </select>
      <div class="muted">{topic.get() === 'core' ? 'Обрано: ядро' : topic.get() === 'all' ? 'Обрано: усе разом' : 'Обрано: інтерфейс'}</div>
      <div class="muted">Спробуйте Tab, Shift+Tab, Space та стрілки.</div>
    </div>
  );
}

function* numbers(): Generator<number> {
  yield 2;
  yield 3;
  yield 5;
}

async function runEngine(write: (value: string) => void): Promise<void> {
  const awaited = await Promise.resolve(7);
  const decoded = JSON.parse('{"name":"Varta","values":[1,2,3]}') as Sample;
  let total = 0;
  for (const value of numbers()) total += value;
  write(`JSON: ${decoded.name}, елементів: ${decoded.values.length}\nГенератор: 2 + 3 + 5 = ${total}\nPromise / await: ${awaited}\nУсі обчислення завершено.`);
  View.commit();
}

function EngineCard(props: CardProps): number {
  const result = signal<string>(props.rt, 'Запустіть приклад: типізований JSON,\nгенератор і Promise з async/await.\nРезультат з’явиться тут.');
  return (
    <div class="card">
      <div class="heading">{'04 / Можливості ядра'}</div>
      <div class="output">{result.get()}</div>
      <button id="run-engine" type="button" class="button primary wide"
        onClick={() => { runEngine(result.set); }}>Запустити приклад</button>
    </div>
  );
}

export function main(): void {
  const rt = new Runtime();
  // Reuse the existing tested L1; TSX bindings subscribe only to what they read.
  View.setEffect((run: () => void): void => { effect(rt, run); });
  View.setTitle('Varta — інтерактивна демонстрація');
  View.stylesheet(1, css);
  View.mount(
    <div id="app" class="app">
      <div class="shell">
        <Header />
        <div class="grid">
          <CounterCard rt={rt} />
          <EditorCard rt={rt} />
          <TypographyCard rt={rt} />
          <EngineCard rt={rt} />
        </div>
        <div class="footer">Колесо миші — прокручування. Змініть розмір вікна, щоб побачити адаптивне компонування.</div>
      </div>
    </div>
  );
}
