// Interface strings of the Single Chat, in one place (Ukrainian, as in the mockups).

export const TIMELINE_LABEL = "Розмова";
export const THINKING = "Міркування";
export const DELEGATED = "Делеговано";
export const COPY = "⧉ Копіювати";

export const APPROVAL_TITLE = "Потрібен ваш дозвіл";
export const APPROVAL_TEXT = "Агент хоче виконати дію, яка потребує підтвердження людиною.";
export const APPROVE = "Дозволити";
export const REJECT = "Відхилити";
export const TOOL = "Інструмент";

export const QUESTION_TITLE = "Агент питає";

export const FAILURE_TITLE = "Запуск не вдався";
export const REJECTED_TITLE = "Відхилено вами";
export const CANCELLED_TITLE = "Зупинено";
export const BLOCKED_TITLE = "Заблоковано політикою";
export const CODE = "Код";

export const STATE_RUNNING = "виконується";
export const STATE_DONE = "завершено";
export const STATE_ERROR = "помилка";
export const STATE_WAITING = "очікує";
export const STATE_NEEDS_ANSWER = "потрібна відповідь";
export const STATE_ANSWERED = "відповідь надіслано";
export const STATE_APPROVED = "дозволено";
export const STATE_REJECTED = "відхилено";
export const STATE_SENDING = "надсилається…";
export const STATE_WRITING = "пише…";
export const STATE_FAILED_TO_SEND = "не надіслано";

export const COMPOSER_LABEL = "Повідомлення";
export const COMPOSER_HINT = "Enter — надіслати · Shift+Enter — новий рядок";
export const SEND = "Надіслати";
export const PLACEHOLDER_PREFIX = "Напишіть ";

// The console's start and the live stream (src/app/console.ts).
export const HEAD_AGENT = "Головний агент";
export const STARTING = "Підключення…";
export const NO_KEY = "Ключ API не збережено.";
export const NO_AGENT = "У просторі немає головного агента.";
export const UNAVAILABLE = "API недоступний: ";
export const STREAM_FAILED = "Сесію не знайдено: ";
export const STREAM_AUTHENTICATE = "Ключ API більше не дійсний.";
export const STREAM_GAVE_UP = "З'єднання втрачено: ";
export const STREAM_REFUSED = "Потік не відкрився: ";
export const STREAM_INVALID = "Недійсний кадр потоку: ";

// The console screen around the chat (src/ui/console.tsx).
export const NOT_CONNECTED = "Не підключено";
export const SPACE_PREFIX = "Простір ";
export const SETTINGS = "Налаштування";
export const CONTEXT = "Контекст";
export const CONTEXT_PANEL = "Контекст розмови";
export const CLOSE_CONTEXT = "Закрити контекст";
export const COMPOSER_DISABLED = "Спершу підключіться до простору";

// The empty chat.
export const EMPTY_TITLE = "Чим допомогти сьогодні?";
export const EMPTY_TEXT = "Головний агент керує всіма агентами простору: делегує задачі, стежить за запусками і питає вашого дозволу на важливі дії.";
/** Each suggestion sends its title as the message; the hint says what it gives. */
export const SUGGESTIONS: { title: string; hint: string }[] = [
  { title: "Що зараз відбувається?", hint: "Активні запуски й агенти, що чекають на мене" },
  { title: "Підсумок за тиждень", hint: "Витрати, успішні й невдалі запуски" },
  { title: "Створи агента для підтримки", hint: "Відповіді клієнтам з бази знань" },
];

// Session stitching: history on scroll and the date line of each segment.
export const OLDER = "Показати раніші розмови";
export const LOADING_OLDER = "Завантаження…";
export const OLDER_FAILED = "Не вдалося завантажити історію: ";
export const TRANSCRIPT_GONE = "Розмову закрито; її історія недоступна.";
/** Month names in the genitive, January first, for the date line ("3 січня 2026"). */
export const MONTHS: string[] = ["січня", "лютого", "березня", "квітня", "травня", "червня", "липня", "серпня", "вересня", "жовтня", "листопада", "грудня"];

// The settings dialog.
export const SETTINGS_TITLE = "Підключення до UARP";
export const BASE_URL_LABEL = "Адреса API";
export const BASE_URL_HELP = "Зазвичай змінювати не потрібно.";
export const API_KEY_LABEL = "Ключ API";
export const API_KEY_PLACEHOLDER = "uarp_…";
export const API_KEY_HELP = "Ключ зберігається лише на цьому пристрої. Створити його можна в налаштуваннях простору на snaga.ai.";
export const SAVE = "Зберегти й підключитись";
export const CANCEL = "Скасувати";
export const FORGET_KEY = "Забути ключ";
export const KEY_MISSING = "Введіть ключ API.";
export const URL_INVALID = "Адреса API має починатися з https:// або http://.";
export const SAVED = "Налаштування збережено.";
export const SAVED_NEXT_START = "Налаштування збережено; вони діятимуть після перезапуску консолі.";
export const SAVE_FAILED = "Не вдалося зберегти налаштування: ";

// The confirm dialog before forgetting the key.
export const FORGET_TITLE = "Забути ключ API?";
export const FORGET_TEXT = "Ключ буде видалено з цього пристрою. Щоб підключитися знову, його доведеться ввести ще раз.";
export const FORGET_ACTION = "Забути";
export const FORGOTTEN = "Ключ видалено з цього пристрою.";
export const FORGET_FAILED = "Не вдалося видалити ключ: ";

// The context panel: agents and runs mentioned in the conversation.
export const AGENTS = "Агенти";
export const RUNS = "Запуски";
export const CONTEXT_METER = "Контекст моделі";
export const NOTHING_MENTIONED = "У розмові ще немає запусків.";
