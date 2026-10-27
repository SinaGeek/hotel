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

async def navigate(ws_url, url):
    async with websockets.connect(ws_url) as ws:
        msg_id = 100
        req = {
            "id": msg_id,
            "method": "Page.navigate",
            "params": {"url": url}
        }
        await ws.send(json.dumps(req))
        await asyncio.sleep(2)

async def main():
    pages = get_pages()
    hms_gen_tr = next((p for p in pages if 'hms.gen.tr' in p.get('url', '')), None)
    if not hms_gen_tr:
        print("hms.gen.tr tab not found")
        return
    ws_url = hms_gen_tr['webSocketDebuggerUrl']
    
    print("Navigating to /raporlar/ ...")
    await navigate(ws_url, 'https://app.hms.gen.tr/raporlar/')
    await asyncio.sleep(2)
    
    reports = await evaluate_js(ws_url, """(() => {
        const reportCards = Array.from(document.querySelectorAll('.portlet, .tile, .list-group-item, a[href*="rapor"]'))
            .map(el => ({
                text: el.innerText.replace(/\\s+/g, ' ').trim(),
                href: el.getAttribute('href') || el.querySelector('a')?.getAttribute('href')
            })).filter(x => x.text && x.href);
        return {
            title: document.title,
            reports: reportCards
        };
    })()""")
    print(json.dumps(reports, indent=2, ensure_ascii=False))

if __name__ == '__main__':
    asyncio.run(main())
