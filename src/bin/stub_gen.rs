fn main() {
    let stub = raydriver::stub_info().unwrap();
    stub.generate().unwrap();
}
