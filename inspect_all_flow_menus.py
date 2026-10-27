import asyncio
import json
import urllib.request
import websockets

async def main():
    pages = json.loads(urllib.request.urlopen('http://localhost:9222/json/list').read())
    p = next(x for x in pages if 'hmsflow.com' in x.get('url',''))
    ws_url = p['webSocketDebuggerUrl']
    async with websockets.connect(ws_url) as ws:
        menu_items_script = '''(() => {
            const btns = Array.from(document.querySelectorAll('header button, nav button'));
            const targets = ['Rezervasyon', 'Tesis', 'Hareketler', 'Dönemsel Fiyat', 'Fatura', 'Acente & Cari', 'Araçlar'];
            const result = {};
            
            // Function to query currently visible menu items
            function getItems() {
                return Array.from(document.querySelectorAll('[role="menu"] [role="menuitem"], [role="menu"] a, div[data-radix-popper-content-wrapper] a, div[data-radix-popper-content-wrapper] button'))
                    .map(el => ({
                        text: el.innerText.trim(),
                        href: el.getAttribute('href') || ''
                    })).filter(x => x.text);
            }
            return { targets: targets };
        })()'''
        
        # We will click each button sequentially with a small delay
        result = {}
        for target in ['Rezervasyon', 'Tesis', 'Hareketler', 'Dönemsel Fiyat', 'Fatura', 'Acente & Cari', 'Araçlar']:
            click_code = f'''(() => {{
                const btns = Array.from(document.querySelectorAll('header button, nav button'));
                const b = btns.find(x => x.innerText.trim() === '{target}');
                if (b) {{
                    b.click();
                    return true;
                }}
                return false;
            }})()'''
            await ws.send(json.dumps({'id': 10, 'method': 'Runtime.evaluate', 'params': {'expression': click_code, 'returnByValue': True}}))
            await asyncio.sleep(0.5)
            
            extract_code = '''(() => {
                return Array.from(document.querySelectorAll('[role="menu"] [role="menuitem"], [role="menu"] a, div[data-radix-popper-content-wrapper] a, div[data-radix-popper-content-wrapper] button'))
                    .map(el => ({
                        text: el.innerText.trim(),
                        href: el.getAttribute('href') || ''
                    })).filter(x => x.text);
            })()'''
            await ws.send(json.dumps({'id': 11, 'method': 'Runtime.evaluate', 'params': {'expression': extract_code, 'returnByValue': True}}))
            while True:
                resp = json.loads(await ws.recv())
                if resp.get('id') == 11:
                    result[target] = resp['result']['result']['value']
                    break
            
            # Click again to close
            await ws.send(json.dumps({'id': 12, 'method': 'Runtime.evaluate', 'params': {'expression': click_code}}))
            await asyncio.sleep(0.3)
            
        print(json.dumps(result, indent=2, ensure_ascii=False))

if __name__ == '__main__':
    asyncio.run(main())
