#[macro_export]
macro_rules! btreemap {
    () => {{
        ::std::collections::BTreeMap::new()
    }};
    ($($key:expr => $val:expr),+ $(,)?) => {{
        let mut m = ::std::collections::BTreeMap::new();
        $(
            m.insert($key, $val);
        )+
        m
    }};
}
