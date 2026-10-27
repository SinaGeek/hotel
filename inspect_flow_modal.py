import asyncio
import json
import urllib.request
import websockets

def get_pages():
    with urllib.request.urlopen('http://localhost:9222/json/list') as response:
        pages = json.loads(response.read().decode())
    return [p for p in pages if p.get('type') == 'page']

async def evaluate_js(ws_url, expr):
    async with websockets.connect(ws_url) as ws:
        msg_id = 1
        req = {
            "id": msg_id,
            "method": "Runtime.evaluate",
            "params": {
                "expression": expr,
                "returnByValue": True,
                "awaitPromise": True
            }
        }
        await ws.send(json.dumps(req))
        while True:
            resp = json.loads(await ws.recv())
            if resp.get('id') == msg_id:
                result = resp.get('result', {}).get('result', {})
                return result.get('value')

async def main():
    pages = get_pages()
    hmsflow = next((p for p in pages if 'hmsflow.com' in p.get('url', '')), None)
    if not hmsflow:
        print("hmsflow tab not found")
        return
    ws_url = hmsflow['webSocketDebuggerUrl']
    
    # Click one "Rezervasyon Yap" button or find modals
    res = await evaluate_js(ws_url, """(() => {
        const btns = Array.from(document.querySelectorAll('button'));
        const rezBtn = btns.find(b => b.innerText.includes('Rezervasyon Yap'));
        if (rezBtn) {
            rezBtn.click();
        }
        return { clicked: !!rezBtn };
    })()""")
    print("Clicked Rezervasyon Yap:", res)
    await asyncio.sleep(1)
    
    # Inspect modal if opened
    modal_info = await evaluate_js(ws_url, """(() => {
        const dialog = document.querySelector('[role="dialog"], .modal, div[class*="dialog"], div[class*="modal"]');
        if (!dialog) return { found: false, body: document.body.innerText.slice(0, 500) };
        const inputs = Array.from(dialog.querySelectorAll('input, select, textarea, button')).map(el => ({
            tag: el.tagName,
            type: el.type,
            placeholder: el.placeholder || '',
            name: el.name || '',
            label: el.closest('label')?.innerText.trim() || el.previousElementSibling?.innerText.trim() || '',
            text: el.innerText.trim()
        }));
        return {
            found: true,
            title: dialog.querySelector('h2, h3, header')?.innerText.trim(),
            inputs: inputs
        };
    })()""")
    print("Modal info:", json.dumps(modal_info, indent=2, ensure_ascii=False))

if __name__ == '__main__':
    asyncio.run(main())
