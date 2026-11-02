export function main():void {
  View.setTitle('Varta select list');
  View.stylesheet(1,'.choice { display:block; width:160px; height:40px; min-width:0; min-height:0; padding:4px; border:2px solid #000; border-radius:0; outline:none; background-color:#fff; color:#00f; font-family:"Varta Fixture 400"; font-size:20px; font-weight:normal; font-style:normal; line-height:20px }');
  const select=View.node();View.createElement(select,26);View.setClass(select,'choice');
  for(let i=0;i<12;i++){
    const option=View.node();View.createElement(option,27);View.setAttribute(option,4,i===0?'blocked':i<3?'same':`${i}`);
    if(i===0)View.setAttribute(option,6,'');
    const label=View.node();View.createText(label,'A'.repeat(i+1));View.insert(option,label,0);View.insert(select,option,0);
  }
  View.on(select,2,(event:ViewEvent):void=>{console.log(`input ${event.value} ${event.seq}`);View.setValue(select,event.value);},0,0,0,0);
  View.on(select,3,(event:ViewEvent):void=>console.log(`change ${event.value} ${event.seq}`),0,0,0,0);
  View.mount(select);
}
