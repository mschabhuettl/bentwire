//! Debug aid: `cargo run -p bentwire-convert --example actinfo -- FILE.ACT` lists payloads and
//! scripts of an actor.
fn main() {
    let p = std::env::args().nth(1).expect("FILE.ACT");
    let a = agent_formats::act::parse(&std::fs::read(&p).unwrap()).unwrap();
    let h = &a.header;
    println!(
        "v{}.{} kind {} rest {} idle {} logical {}x{} px {}x{}",
        a.major,
        a.minor,
        h.kind,
        h.rest_pose,
        h.idle_anim,
        h.logical_w,
        h.logical_h,
        h.default_px_w,
        h.default_px_h
    );
    for (i, pl) in a.payloads.iter().enumerate() {
        match pl {
            agent_formats::act::Payload::Wmf(w) => {
                let f: Vec<String> = w
                    .records
                    .iter()
                    .map(|r| format!("{:04x}", r.function))
                    .collect();
                println!("{i}: wmf bbox {:?} records {}", w.bbox, f.join(" "));
            }
            agent_formats::act::Payload::Composite(c) => println!("{i}: composite {c:?}"),
            agent_formats::act::Payload::Bitmap(b) => {
                println!("{i}: bitmap {}x{}", b.width, b.height)
            }
        }
    }
    for g in &a.animations {
        for v in &g.variants {
            println!("anim {} {:?}", g.id, v.commands);
        }
    }
}
