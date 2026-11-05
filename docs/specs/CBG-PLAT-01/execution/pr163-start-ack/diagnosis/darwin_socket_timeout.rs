use std::{io::{Read,Write},net::{TcpListener,TcpStream},thread,time::{Duration,Instant}};
fn main() {
 let listener=TcpListener::bind("127.0.0.1:0").unwrap();
 let addr=listener.local_addr().unwrap();
 let server=thread::spawn(move || { let (mut s,_)=listener.accept().unwrap(); let mut byte=[0]; s.read_exact(&mut byte).unwrap(); thread::sleep(Duration::from_millis(400)); let _=s.write_all(b"OK\n"); });
 let mut stream=TcpStream::connect_timeout(&addr,Duration::from_millis(250)).unwrap();
 stream.set_read_timeout(Some(Duration::from_millis(250))).unwrap();
 stream.set_write_timeout(Some(Duration::from_millis(250))).unwrap();
 stream.write_all(b"S").unwrap();
 let began=Instant::now(); let mut byte=[0];
 match stream.read(&mut byte) { Ok(n)=>println!("read={n}"),Err(e)=>println!("display={e}; kind={:?}; errno={:?}; elapsed_ms={}",e.kind(),e.raw_os_error(),began.elapsed().as_millis()) }
 server.join().unwrap();
}
