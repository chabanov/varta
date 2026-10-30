type ResponseType = 'json' | 'binary' | 'text' | 'void';
interface Spec {
  responseType?: ResponseType;
}
function decode(type: ResponseType): string {
  return `decode ${type}`;
}
function run(spec: Spec): string {

  return decode(spec.responseType ?? 'json');
}
export async function main(): Promise<void> {
  const spec: Spec = {};
  console.log(decode(spec.responseType ?? 'json'));
  console.log(run({ responseType: 'void' }));
}
