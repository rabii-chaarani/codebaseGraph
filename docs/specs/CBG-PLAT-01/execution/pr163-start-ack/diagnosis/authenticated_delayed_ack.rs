use std::{io::{Read,Write},net::{TcpListener,TcpStream},thread,time::{Duration,Instant}};
fn read_frame(stream: &mut TcpStream, deadline: Instant) -> Result<Vec<u8>,String> {
 let mut frame=Vec::new(); let mut byte=[0u8];
 while frame.len()<4096 { let remaining=deadline.checked_duration_since(Instant::now()).ok_or("guardian frame deadline expired")?; if remaining.is_zero(){return Err("guardian frame deadline expired".into())} stream.set_read_timeout(Some(remaining)).map_err(|e|e.to_string())?;
 match stream.read(&mut byte){Ok(0)=>return Err("guardian control stream closed before frame".into()),Ok(_)=>{if byte[0]==b'\n'{return Ok(frame)}frame.push(byte[0])},Err(e) if e.kind()==std::io::ErrorKind::Interrupted=>continue,Err(e)=>return Err(e.to_string())}}
 Err("guardian frame exceeds limit".into())
}
fn check(ack_budget:Duration) {
 let listener=TcpListener::bind("127.0.0.1:0").unwrap(); let addr=listener.local_addr().unwrap();
 let payload=br#"{"nonce":"test-capability","command":"START"}"#;
 let server=thread::spawn(move || {let(mut s,_)=listener.accept().unwrap();let got=read_frame(&mut s,Instant::now()+Duration::from_millis(100)).unwrap();assert_eq!(got,payload);let commits=1;thread::sleep(Duration::from_millis(350));let ack=s.write_all(b"OK\n");println!("server authenticated=true committed={commits} ack={ack:?}");commits});
 let mut s=TcpStream::connect_timeout(&addr,Duration::from_millis(250)).unwrap();s.set_write_timeout(Some(Duration::from_millis(250))).unwrap();s.write_all(payload).unwrap();s.write_all(b"\n").unwrap();let began=Instant::now();let result=read_frame(&mut s,Instant::now()+ack_budget);println!("ack_budget_ms={} elapsed_ms={} result={result:?}",ack_budget.as_millis(),began.elapsed().as_millis());assert_eq!(server.join().unwrap(),1);
}
fn main(){check(Duration::from_millis(250));check(Duration::from_secs(5));}
