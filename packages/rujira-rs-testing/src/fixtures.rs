use anybuf::Anybuf;
use cosmwasm_std::Binary;

pub fn mock_pool_btc() -> Binary {
    Anybuf::new()
        .append_string(1, "1000")
        .append_string(2, "2000")
        .append_message(3, &Anybuf::new())
        .append_string(4, "3000")
        .append_int32(5, 1)
        .append_int64(6, 6)
        .append_string(7, "0")
        .append_string(8, "0")
        .append_string(9, "0")
        .append_int64(10, 1_000_000)
        .into_vec()
        .into()
}
