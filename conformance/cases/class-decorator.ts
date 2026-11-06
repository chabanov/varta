const registered: string[] = [];
function register(value: abstract new () => unknown, context: ClassDecoratorContext): void {
  registered.push(String(context.name));
}
@register
class Service {}
export function main(): void {
  console.log(registered.join(','));
}
