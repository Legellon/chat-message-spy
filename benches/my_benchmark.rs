use chat_spy::match_pattern::{MatchMode, MatchPattern};
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn criterion_benchmark(c: &mut Criterion) {
    let mut p = MatchPattern::from([
        "Kek",
        "jopa",
        "jejejeje",
        "underasd",
        "sdfsdfsd",
        "sasdasd",
        "sdfsfsf",
        "sdfsssfsfsdf",
        "fsfsfsfsfdsf",
        "ssffsfsfsfs",
        "sdfs",
    ]);
    p.set_mode(MatchMode::Inclusive);
    let s = "Some sTrIng to bench format_word fn sdsdfsdf sdf sdf sdf sdf sdf sdf sdfdsdssdfdsdf Hfjdfhdsfods Ssido sa dfdkds KSsdkf nsdf jksdf sdlkf sdf sdf ".to_owned();
    c.bench_function("format_word", |b| {
        b.iter(|| {
            for _ in 0..30_000 {
                black_box(p.match_str(&s));
            }
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
