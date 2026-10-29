import argparse, datetime, http.server, json, pathlib, socketserver, threading
parser=argparse.ArgumentParser(); parser.add_argument('--ready',required=True); parser.add_argument('--requests',required=True)
args=parser.parse_args(); log=pathlib.Path(args.requests); guard=threading.Lock()
class Handler(http.server.BaseHTTPRequestHandler):
    def reject(self):
        row={'at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'method':self.command,'target':self.path,'client':self.client_address}
        with guard:
            with log.open('a',encoding='utf-8') as f: f.write(json.dumps(row)+'\n'); f.flush()
        self.send_response(502,'Qualification-Network-Disabled'); self.send_header('Content-Length','0'); self.end_headers()
    do_CONNECT=reject
    do_GET=reject
    do_POST=reject
    def log_message(self,*args): pass
class Server(socketserver.ThreadingMixIn,http.server.HTTPServer):
    daemon_threads=True
server=Server(('127.0.0.1',0),Handler)
pathlib.Path(args.ready).write_text(json.dumps({'port':server.server_port,'pid':__import__('os').getpid(),'at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'policy':'loopback only, always502, no forwarding code'}),encoding='utf-8')
print('READY '+str(server.server_port),flush=True)
server.serve_forever()
