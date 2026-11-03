// Loopback-only development pages. Never serve the project or repository root.
import {createServer} from 'node:http';
import {open,lstat,realpath} from 'node:fs/promises';
import {constants} from 'node:fs';
import {dirname,join,resolve,sep} from 'node:path';
import {fileURLToPath} from 'node:url';
import {MOCKUPS} from '../mockups/official-host.js';

const repository=resolve(dirname(fileURLToPath(import.meta.url)),'../../..');
const prefix='/uarp-console/mockups/';
const pages=[...MOCKUPS,'console.css','frame.css','lint.html','official-host.js',
  'lint/lint-html.js','conformance/conformance.html','conformance/conformance-frame.html',
  'conformance/mockup-batch.js','conformance/compare.js'];
const modules=['view-dom','view-model','view-codec','view-css','view-fonts','view-select','view-policy','view-variables','view-custom-properties','view-expansion'];
export const MAX_STATIC_BYTES=1024*1024;

export function createMockupServer({root=repository}={}){
  const assets=new Map(pages.map(name=>[prefix+name,join(root,'examples/uarp-console/mockups',name)]));
  for(const name of modules)assets.set(`/crates/varta-project/src/browser/${name}.mjs`,join(root,'crates/varta-project/src/browser',`${name}.mjs`));
  assets.set('/crates/varta-common/browser/registry.mjs',join(root,'crates/varta-common/browser/registry.mjs'));
  const server=createServer(async(request,response)=>{
    let handle;
    try{
      const port=server.address()?.port;
      if(![`127.0.0.1:${port}`,`localhost:${port}`].includes(request.headers.host)){
        response.writeHead(403).end();return;
      }
      if(!['GET','HEAD'].includes(request.method)){response.writeHead(405,{Allow:'GET, HEAD'}).end();return;}
      const url=new URL(request.url,'http://127.0.0.1');
      const name=decodeURIComponent(url.pathname);
      if(name==='/'){response.writeHead(302,{Location:prefix+'lint.html'}).end();return;}
      const path=assets.get(name);
      if(!path){response.writeHead(404).end();return;}
      // Only named public assets are addressable; symlinks must not turn one
      // of those names into an environment file or another private document.
      const actual=await realpath(path),base=await realpath(root);
      if(!actual.startsWith(base+sep)||(await lstat(path)).isSymbolicLink()){
        response.writeHead(404).end();return;
      }
      handle=await open(path,constants.O_RDONLY|(constants.O_NOFOLLOW??0));
      const stat=await handle.stat();
      if(!stat.isFile()){response.writeHead(404).end();return;}
      if(stat.size>MAX_STATIC_BYTES){response.writeHead(413).end();return;}
      const bytes=await handle.readFile();
      if(bytes.length>MAX_STATIC_BYTES){response.writeHead(413).end();return;}
      response.writeHead(200,{
        'Content-Type':name.endsWith('.html')?'text/html; charset=utf-8':name.endsWith('.css')?'text/css; charset=utf-8':'text/javascript; charset=utf-8',
        'Content-Length':bytes.length,'Cache-Control':'no-store','X-Content-Type-Options':'nosniff',
        'Content-Security-Policy':"default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; connect-src 'self'; frame-src 'self'; frame-ancestors 'self'; object-src 'none'; base-uri 'none'; form-action 'none'",
      });
      response.end(request.method==='HEAD'?undefined:bytes);
    }catch(error){response.writeHead(error instanceof URIError?400:404).end();}
    finally{await handle?.close();}
  });
  return server;
}

if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  const port=Number(process.argv[2]??8766);
  if(!Number.isInteger(port)||port<0||port>65535)throw new Error('port must be an integer from 0 to 65535');
  const server=createMockupServer();server.listen(port,'127.0.0.1',()=>console.log(`Mockups: http://127.0.0.1:${server.address().port}${prefix}lint.html`));
}
