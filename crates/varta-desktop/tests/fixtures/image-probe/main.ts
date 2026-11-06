// The image probe: one packaged PNG (`icons/red.png`, declared in the
// configuration's `images`) as a 40x20 block image with a rounded content
// box and an alternative text, drawn by the native renderer.
export function main(): void {
  View.setTitle('Varta image probe');
  View.stylesheet(1, '.picture { display:block; width:40px; height:20px; border-radius:4px }');
  const image = View.node();
  // Tag 30 is `img`; attributes 1, 13 and 14 are `id`, `src` and `alt`.
  View.createElement(image, 30);
  View.setAttribute(image, 1, 'picture');
  View.setAttribute(image, 13, 'icons/red.png');
  View.setAttribute(image, 14, 'Red square');
  View.setClass(image, 'picture');
  View.insert(1, image, 0);
  View.commit();
  console.log('image ready');
}
