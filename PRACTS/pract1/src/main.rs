use derive_more::Display;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
#[display("OrderId({})", _0)]
pub struct OrderId(u64);

impl OrderId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Display)]
#[display("{}₴", *_0 as f64 / 100.0)]
pub struct AmountKopecks(u64);

impl AmountKopecks {
    pub fn from_hryvnias(hrn: u64) -> Self {
        Self(hrn * 100)
    }
}

#[derive(Debug, Clone, Display)]
#[display("{}", _0)]
pub struct ShippingAddress(String);

impl ShippingAddress {
    pub fn new(addr: impl Into<String>) -> Self {
        Self(addr.into())
    }
}

#[derive(Debug)]
pub struct New;

#[derive(Debug)]
pub struct Paid {
    pub amount: AmountKopecks,
}

#[derive(Debug)]
pub struct Shipped {
    pub amount: AmountKopecks,
    pub address: ShippingAddress,
}

#[derive(Debug)]
pub struct Delivered {
    pub amount: AmountKopecks,
    pub address: ShippingAddress,
}

#[derive(Debug)]
pub struct Cancelled {
    pub reason: String,
}

#[derive(Debug)]
pub struct Order<State> {
    pub id: OrderId,
    pub state: State,
}

impl Order<New> {
    pub fn new(id: OrderId) -> Self {
        println!("[{}] Order created", id);
        Self { id, state: New }
    }

    pub fn pay(self, amount: AmountKopecks) -> Order<Paid> {
        println!("[{}] Paid {}", self.id, amount);
        Order {
            id: self.id,
            state: Paid { amount },
        }
    }

    pub fn cancel(self, reason: impl Into<String>) -> Order<Cancelled> {
        let reason = reason.into();
        println!("[{}] Cancelled: {}", self.id, reason);
        Order {
            id: self.id,
            state: Cancelled { reason },
        }
    }
}

impl Order<Paid> {
    pub fn ship(self, address: ShippingAddress) -> Order<Shipped> {
        println!("[{}] Shipped to «{}»", self.id, address);
        Order {
            id: self.id,
            state: Shipped {
                amount: self.state.amount,
                address,
            },
        }
    }

    pub fn cancel(self, reason: impl Into<String>) -> Order<Cancelled> {
        let reason = reason.into();
        println!(
            "[{}] Cancelled after payment, refund {} (reason: {})",
            self.id, self.state.amount, reason
        );
        Order {
            id: self.id,
            state: Cancelled { reason },
        }
    }
}

impl Order<Shipped> {
    pub fn deliver(self) -> Order<Delivered> {
        println!("[{}] Delivered to «{}»", self.id, self.state.address);
        Order {
            id: self.id,
            state: Delivered {
                amount: self.state.amount,
                address: self.state.address,
            },
        }
    }
}

impl Order<Delivered> {
    pub fn summary(&self) {
        println!(
            "[{}] Delivered to «{}», paid {}",
            self.id, self.state.address, self.state.amount
        );
    }
}

impl Order<Cancelled> {
    pub fn summary(&self) {
        println!("[{}] Cancelled: {}", self.id, self.state.reason);
    }
}

fn main() {
    println!("=== Scenario 1: full success path ===");
    let order = Order::new(OrderId::new(1));
    let order = order.pay(AmountKopecks::from_hryvnias(1500));
    let order = order.ship(ShippingAddress::new("Khreschatyk st. 1, Kyiv"));
    let order = order.deliver();
    order.summary();

    println!();
    println!("=== Scenario 2: cancel before payment ===");
    let order = Order::new(OrderId::new(2));
    let order = order.cancel("customer changed mind");
    order.summary();

    println!();
    println!("=== Scenario 3: cancel after payment ===");
    let order = Order::new(OrderId::new(3));
    let order = order.pay(AmountKopecks::from_hryvnias(300));
    let order = order.cancel("out of stock");
    order.summary();
}
