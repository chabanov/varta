class Settings {
  accessor volume = 5;
  accessor theme: string = 'light';
}
export function main(): void {
  const settings = new Settings();
  settings.volume += 3;
  settings.theme = settings.theme + '-' + String(settings.volume);
  console.log(settings.theme + ' ' + String(settings.volume));
}
