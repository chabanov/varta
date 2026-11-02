/** A host callback is invoked only between turns (or before instantiation).
 * Abort wins over late answers; each instance owns at most one open callback. */
export function createPromptDriver({ask,signal,waiting=()=>{}}) {
  let active=false;
  return(info)=>{
    if(active)return Promise.reject(new Error('permission prompt already open'));
    if(signal.aborted)return Promise.reject(new Error('permission prompt aborted'));
    active=true;
    return new Promise((resolve,reject)=>{
      let ended=false;
      const finish=(error,answer)=>{
        if(ended)return;ended=true;signal.removeEventListener('abort',abort);
        if(error)reject(error);else resolve(answer);
      };
      const abort=()=>finish(new Error('permission prompt aborted'));
      signal.addEventListener('abort',abort,{once:true});
      try{waiting(true);Promise.resolve(ask(info,{signal})).then(answer=>finish(null,answer),error=>finish(error));}catch(error){finish(error);}
    }).finally(()=>{active=false;waiting(false);});
  };
}

/** Host-owned consent UI, outside the guest's closed view. All package text is
 * textContent; style and action labels belong exclusively to this host module. */
export function createPermissionPrompt({document=globalThis.document}={}) {
  if(!document?.body)throw new TypeError('permission prompt requires a document');
  let active=false;
  return(info,{signal})=>new Promise((resolve,reject)=>{
    if(active){reject(new Error('permission prompt already open'));return;}
    if(signal.aborted){reject(new Error('permission prompt aborted'));return;}
    active=true;let shell,dialog,ended=false;
    const finish=(error,answer)=>{
      if(ended)return;ended=true;active=false;signal.removeEventListener('abort',abort);
      if(dialog?.open)dialog.close();shell?.remove();if(error)reject(error);else resolve(answer);
    };
    const abort=()=>finish(new Error('permission prompt aborted'));
    try{
      shell=document.createElement('div');shell.setAttribute('data-varta-permission-prompt','');document.body.append(shell);
      const shadow=shell.attachShadow({mode:'closed'}),sheet=new document.defaultView.CSSStyleSheet();
      sheet.replaceSync(':host{all:initial}dialog{box-sizing:border-box;max-width:min(38rem,90vw);max-height:85vh;overflow:auto;border:1px solid #777;border-radius:12px;padding:20px;background:white;color:#111;font:16px/1.5 system-ui}dialog::backdrop{background:#0008}h2{margin:0 0 12px;font-size:22px}p{margin:8px 0}code{overflow-wrap:anywhere}blockquote{white-space:pre-wrap;border-inline-start:3px solid #bbb;margin:8px 0;padding-inline-start:12px}footer{display:flex;gap:12px;flex-wrap:wrap;margin-top:16px}button{font:inherit;padding:8px 16px;cursor:pointer}button:focus-visible{outline:3px solid #265dcc;outline-offset:2px}');shadow.adoptedStyleSheets=[sheet];
      dialog=document.createElement('dialog');dialog.setAttribute('aria-labelledby','permission-heading');shadow.append(dialog);
      const add=(tag,text)=>{const element=document.createElement(tag);element.textContent=text;dialog.append(element);return element;};
      add('h2',info.activation?'Open external link':'Application permission').id='permission-heading';
      add('p',info.activation?'Open this address once. Future activations ask again.':info.required?'This permission is required to start the application.':'The application requests this permission.');
      add('code',info.capability);add('p','Application identity:');add('code',info.artifact);
      if(info.scope.length){add('p','Requested resources:');const list=add('ul','');for(const pattern of info.scope){const item=document.createElement('li');item.textContent=pattern;list.append(item);}}
      if(info.purpose!==null){add('p','The application says:');add('blockquote',info.purpose);}
      add('p',info.activation?'This choice is not saved.':'Your choice applies only within the host’s allowed scope.');
      const actions=add('footer','');let deny;
      for(const[answer,label]of[['deny','Deny'],['dismiss','Dismiss'],['allow','Allow']]){
        const button=document.createElement('button');button.type='button';button.textContent=answer==='allow'&&info.activation?'Open once':label;button.setAttribute('data-decision',answer);
        button.addEventListener('click',event=>{if(event.isTrusted)finish(null,answer);});actions.append(button);if(answer==='deny')deny=button;
      }
      dialog.addEventListener('cancel',event=>{event.preventDefault();if(event.isTrusted)finish(null,'dismiss');});
      signal.addEventListener('abort',abort,{once:true});dialog.showModal();deny.focus({preventScroll:true});
    }catch(error){finish(error);}
  });
}
