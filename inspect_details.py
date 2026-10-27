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
    hms_gen_tr = next((p for p in pages if 'hms.gen.tr' in p.get('url', '')), None)
    hmsflow = next((p for p in pages if 'hmsflow.com' in p.get('url', '')), None)

    if hms_gen_tr:
        ws_url = hms_gen_tr['webSocketDebuggerUrl']
        print("=== HMS.GEN.TR Top Header / Settings / Dropdowns ===")
        header_data = await evaluate_js(ws_url, """(() => {
            const headerLinks = Array.from(document.querySelectorAll('.page-header a, .top-menu a, .dropdown-menu a'))
                .map(a => ({
                    text: a.innerText.replace(/\\s+/g, ' ').trim(),
                    href: a.getAttribute('href')
                })).filter(x => x.text && x.href);
            return headerLinks;
        })()""")
        print(json.dumps(header_data, indent=2, ensure_ascii=False))

    if hmsflow:
        ws_url = hmsflow['webSocketDebuggerUrl']
        print("=== HMSFLOW Layout & Design System ===")
        flow_data = await evaluate_js(ws_url, """(() => {
            const computed = window.getComputedStyle(document.body);
            const primaryBtn = document.querySelector('button, [role="button"]');
            const primaryBtnStyle = primaryBtn ? window.getComputedStyle(primaryBtn) : null;
            const nav = document.querySelector('nav, aside, header');
            const navStyle = nav ? window.getComputedStyle(nav) : null;
            return {
                bodyBg: computed.backgroundColor,
                color: computed.color,
                fontFamily: computed.fontFamily,
                primaryBtn: primaryBtnStyle ? {
                    bg: primaryBtnStyle.backgroundColor,
                    color: primaryBtnStyle.color,
                    borderRadius: primaryBtnStyle.borderRadius,
                    fontFamily: primaryBtnStyle.fontFamily
                } : null,
                nav: navStyle ? {
                    bg: navStyle.backgroundColor,
                    color: navStyle.color,
                    border: navStyle.borderColor
                } : null,
                allButtons: Array.from(document.querySelectorAll('button')).map(b => b.innerText.trim()).filter(Boolean),
                headings: Array.from(document.querySelectorAll('h1, h2, h3, h4')).map(h => ({ tag: h.tagName, text: h.innerText.trim() }))
            };
        })()""")
        print(json.dumps(flow_data, indent=2, ensure_ascii=False))

if __name__ == '__main__':
    asyncio.run(main())
