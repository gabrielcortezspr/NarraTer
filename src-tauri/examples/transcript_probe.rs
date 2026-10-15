//! Lê a resposta de um pedido num transcript do Claude, como o `ask` faz (O5).
//! Uso: cargo run --example transcript_probe -- ARQUIVO.jsonl "início da mensagem entregue"
use narrater_lib::transcript::{answer_in, Found};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [file, msg] = &args[..] else {
        eprintln!("uso: transcript_probe ARQUIVO.jsonl MENSAGEM");
        std::process::exit(2);
    };
    let text = std::fs::read_to_string(file).expect("ler transcript");
    let needle: String = msg.split_whitespace().collect::<Vec<_>>().join(" ");
    match answer_in(&text, &needle) {
        Found::Answer(a) => println!("{}", a),
        Found::Pending => println!("<pendente>"),
        Found::Missing => println!("<não encontrado>"),
    }
}
