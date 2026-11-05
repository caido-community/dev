fn main() {
    std::process::exit(caido_dev::run(std::env::args().skip(1).collect()));
}
