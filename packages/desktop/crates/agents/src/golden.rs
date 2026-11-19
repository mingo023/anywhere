macro_rules! golden_text {
    ($name:literal) => {
        include_str!(concat!("../../../../pocketd/internal/proto/testdata/golden/", $name))
    };
}

macro_rules! golden {
    ($name:literal) => {
        serde_json::from_str::<Value>(include_str!(concat!("../../../../pocketd/internal/proto/testdata/golden/", $name))).unwrap()
    };
}
