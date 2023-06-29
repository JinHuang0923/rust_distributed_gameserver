use rand::Rng;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize)]
pub struct Vector2 {
    pub x: f32,
    pub y: f32,
}
// 二维向量的方向快速创建实现
impl Vector2 {
    pub fn new(x: f32, y: f32) -> Vector2 {
        Vector2 { x, y }
    }
    pub fn zero() -> Vector2 {
        Vector2 { x: 0.0, y: 0.0 }
    }
    pub fn one() -> Vector2 {
        Vector2 { x: 1.0, y: 1.0 }
    }

    pub fn up() -> Vector2 {
        Vector2 { x: 0.0, y: 1.0 }
    }
    pub fn down() -> Vector2 {
        Vector2 { x: 0.0, y: -1.0 }
    }
    pub fn left() -> Vector2 {
        Vector2 { x: -1.0, y: 0.0 }
    }
    pub fn right() -> Vector2 {
        Vector2 { x: 1.0, y: 0.0 }
    }
    //随机方向(在以上的基础上下左右方向中随机)
    pub fn random() -> Vector2 {
        let rand_direction = vec![
            Vector2::up(),
            Vector2::down(),
            Vector2::left(),
            Vector2::right(),
        ];
        let rand_index = rand::random::<usize>() % rand_direction.len();
        rand_direction[rand_index]
    }
    pub fn is_quals_zero(&self) -> bool {
        self.x == 0.0 && self.y == 0.0
    }
}
//速度
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct Velocity {
    pub direction: Vector2,
    pub len: f32, // 速度大小
}
impl Velocity {
    pub fn new(direction: Vector2, len: f32) -> Velocity {
        Velocity { direction, len }
    }
    //随机方向,速度大小为10m/s
    pub fn random() -> Velocity {
        Velocity {
            direction: Vector2::random(),
            len: 10.0,
        }
    }
}

///
/// 矩形范围
#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct RectScope {
    pub left_up: Vector2,
    pub right_down: Vector2,
}
impl RectScope {
    pub fn to_full_rect_scope(&self) -> FullRectScope {
        FullRectScope {
            left_up: self.left_up,
            left_down: Vector2::new(self.left_up.x, self.right_down.y),
            right_up: Vector2::new(self.right_down.x, self.left_up.y),
            right_down: self.right_down,
        }
    }
}
///四个顶点坐标的矩形范围
pub struct FullRectScope {
    pub left_up: Vector2,
    pub left_down: Vector2,
    pub right_up: Vector2,
    pub right_down: Vector2,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub position: Vector2,
    pub velocity: Velocity,
    pub money: u64,
}
// user new
impl User {
    pub fn new(id: i64, position: Vector2, velocity: Velocity, money: u64) -> User {
        User {
            id,
            position,
            velocity,
            money,
        }
    }
    //shift, time 移动的时间(单位:s) 移动也是改变世界状态了 需要加锁
    pub fn shift(&mut self, time: f32) {
        if self.velocity.direction.is_quals_zero() {
            return;
        }
        //打印些可观测的信息
        let shift_x = self.velocity.direction.x * self.velocity.len * time;
        let shift_y = self.velocity.direction.y * self.velocity.len * time;
        self.position.x += shift_x;
        self.position.y += shift_y;

        if self.position.x > 1000000.0 {
            self.position.x = 1000000.0;
        }
        if self.position.x < -1000000.0 {
            self.position.x = -1000000.0;
        }
        if self.position.y > 1000000.0 {
            self.position.y = 1000000.0;
        }
        if self.position.y < -1000000.0 {
            self.position.y = -1000000.0;
        }
        // println!(
        //     "user id:{} shift to x:{} y:{}",
        //     self.id, self.position.x, self.position.y
        // );
    }
    //创建一个在大世界中位置随机的用户,速度大小10m/s,移动方向随机,money为1000
    pub fn random(id: i64) -> User {
        User {
            id,
            position: rand_position(),
            velocity: Velocity::random(),
            money: 1000,
        }
    }
    pub fn random_without_velocity(id: i64) -> Self {
        User {
            id,
            position: rand_position(),
            velocity: Velocity::new(Vector2::zero(), 0.0),
            money: 0,
        }
    }
}
#[inline]
pub fn rand_position() -> Vector2 {
    let mut rand = rand::thread_rng();

    //随机位置 x坐标范围为-1000000~1000000 y坐标范围为-1000000~1000000
    Vector2::new(
        rand.gen_range(-1000000.0..1000000.0),
        rand.gen_range(-1000000.0..1000000.0),
    )
}
