import {STATE_ATTRIBUTES} from '../official-host.js';

export const TOLERANCE=1;
const attributes=element=>[...element.attributes]
  .filter(attribute=>!(STATE_ATTRIBUTES[element.localName]??[]).includes(attribute.name))
  .map(attribute=>attribute.name==='style'
    ?`style=${[...element.style].sort().map(name=>`${name}:${element.style.getPropertyValue(name)}`).join(';')}`
    :`${attribute.name}=${attribute.value}`).sort().join(' ');

/** Compare a static HTML reference to its official-host projection. The host
 * root is embedding state. Dialog/fixed subtrees retain structural/state checks
 * but have no box-parity claim because the reference is not in the top layer. */
export function compareMockup(reference,rendered,{layout=true}={}){
  const errors=[],counts={elements:0,boxes:0,fields:0};
  const ra=reference.getBoundingClientRect(),rb=rendered.getBoundingClientRect();
  const children=node=>[...node.childNodes].filter(child=>child.nodeType===1||(child.nodeType===3&&child.data!==''));
  const style=element=>element.ownerDocument.defaultView.getComputedStyle(element);
  const visit=(a,b,path,layout)=>{
    const left=children(a),right=children(b);
    if(left.length!==right.length){errors.push(`${path}: ${left.length} vs ${right.length} children`);return;}
    for(let i=0;i<left.length;i++){
      const x=left[i],y=right[i],here=`${path}>${x.nodeType===1?x.localName+(x.className?'.'+String(x.className).split(' ')[0]:''):'#text'}`;
      if(x.nodeType!==y.nodeType){errors.push(`${here}: node type differs`);continue;}
      if(x.nodeType===3){if(x.data!==y.data)errors.push(`${here}: text differs`);continue;}
      counts.elements++;
      if(x.localName!==y.localName||attributes(x)!==attributes(y)){errors.push(`${here}: element or attributes differ`);continue;}
      if(['input','textarea','select'].includes(x.localName)){
        counts.fields++;
        if(x.value!==y.value)errors.push(`${here}: field value differs`);
        if(x.localName==='input'&&['radio','checkbox'].includes(x.type)&&x.checked!==y.checked)errors.push(`${here}: checked state differs`);
        if(x.localName==='select'&&x.selectedIndex!==y.selectedIndex)errors.push(`${here}: selected option differs`);
      }
      if(x.localName==='dialog'&&x.open!==y.open)errors.push(`${here}: dialog state differs`);
      const checkLayout=layout&&style(x).position!=='fixed'&&x.localName!=='dialog';
      if(checkLayout){
        counts.boxes++;
        const bx=x.getBoundingClientRect(),by=y.getBoundingClientRect();
        const delta=Math.max(Math.abs(bx.left-ra.left-(by.left-rb.left)),Math.abs(bx.top-ra.top-(by.top-rb.top)),Math.abs(bx.width-by.width),Math.abs(bx.height-by.height));
        if(delta>TOLERANCE)errors.push(`${here}: box differs by ${delta.toFixed(1)} px`);
      }
      // Textarea source children represent its initial value, not L0 children.
      if(x.localName!=='textarea')visit(x,y,here,checkLayout);
    }
  };
  visit(reference,rendered,'root',layout);
  return {errors,counts};
}
