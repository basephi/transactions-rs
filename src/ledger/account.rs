use rust_decimal::Decimal;
use serde::Serialize;

const CURRENCY_DECIMAL_SCALE: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Account {
    pub(crate) client: u16,
    pub(crate) available: Decimal,
    pub(crate) held: Decimal,
    pub(crate) total: Decimal,
    pub(crate) locked: bool,
}

impl Account {
    pub fn new(client: u16) -> Self {
        Self {
            client,
            available: Decimal::new(0, CURRENCY_DECIMAL_SCALE),
            held: Decimal::new(0, CURRENCY_DECIMAL_SCALE),
            total: Decimal::new(0, CURRENCY_DECIMAL_SCALE),
            locked: false,
        }
    }
}
