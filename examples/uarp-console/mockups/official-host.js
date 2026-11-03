// Application-owned preview tools over the official host and common registry.
// No copied wire tables, attribute grammars or prototype applier are used here.
import {registry} from '../../../crates/varta-common/browser/registry.mjs';
import {encodeViewBatch} from '../../../crates/varta-project/src/browser/view-codec.mjs';
import {createViewPolicy} from '../../../crates/varta-project/src/browser/view-policy.mjs';
export {createViewDom} from '../../../crates/varta-project/src/browser/view-dom.mjs';
export {createViewModel} from '../../../crates/varta-project/src/browser/view-model.mjs';
export {CssError,createValueMatcher,checkStyleDeclaration,checkStylesheet} from '../../../crates/varta-project/src/browser/view-css.mjs';
export {encodeViewBatch};

export const view=registry.view;
export const MOCKUPS=['chat-empty.html','chat.html','chat-cards.html','chat-context.html','settings.html'];
export const OPEN_URL_PATTERNS=['https://docs.snaga.ai/*'];
export const STATE_ATTRIBUTES={dialog:['open'],input:['value','checked'],textarea:[]};
export const wellFormed=text=>!text.includes('\0')&&text===text.toWellFormed();
export function linkPolicy(patterns=OPEN_URL_PATTERNS){
  return createViewPolicy({capabilities:[{id:view.capabilities.openUrl,patterns}]}).linkRequested;
}
export function batchEncoder(){
  const operations=[];
  const builder={op(name,fields){operations.push({name,...fields});return builder;},encode(){return encodeViewBatch(operations);}};
  return builder;
}
