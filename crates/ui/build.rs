// Translations are embedded at compile time; rebuild when they change.
fn main() {
    println!("cargo:rerun-if-changed=../../locales");
}
