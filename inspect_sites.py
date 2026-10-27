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
    for p in pages:
        print(f"=== Page: {p.get('title')} ({p.get('url')}) ===")
        ws_url = p.get('webSocketDebuggerUrl')
        if not ws_url:
            continue
        try:
            links = await evaluate_js(ws_url, """(() => {
                const navLinks = Array.from(document.querySelectorAll('nav a, header a, aside a, .sidebar a, [role="navigation"] a, ul.page-sidebar-menu a'))
                    .map(a => ({
                        text: a.innerText.trim(),
                        href: a.getAttribute('href')
                    })).filter(x => x.text && x.href);
                return {
                    url: window.location.href,
                    title: document.title,
                    navCount: navLinks.length,
                    navLinks: navLinks
                };
            })()""")
            print(json.dumps(links, indent=2, ensure_ascii=False))
        except Exception as e:
            print(f"Error evaluating on {p.get('url')}: {e}")

if __name__ == '__main__':
    asyncio.run(main())
