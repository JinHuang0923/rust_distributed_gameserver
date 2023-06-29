
use common::basic::Vector2;
use serde::{Serialize, Deserialize};

#[derive(Hash, Eq, PartialEq, Debug, Clone, Copy, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MapTag {
    pub field_id: char, //分区只是为了快速区别第几象限,供人类可读
    pub x_len: i32,     //(-5 -- 5)
    pub y_len: i32,     // (-5 -- 5)
}
impl MapTag{
    pub fn to_string(&self) -> String {
        format!("{}:{}:{}", self.field_id, self.x_len, self.y_len)
    }
    pub fn from_string(s: &str) -> MapTag {
        let v: Vec<&str> = s.split(":").collect();
        let field_id = v[0].chars().next().unwrap();
        let x_len = v[1].parse::<i32>().unwrap();
        let y_len = v[2].parse::<i32>().unwrap();
        MapTag {
            field_id,
            x_len,
            y_len,
        }
    }
}
pub fn pos2map_tag(area_len: u32, pos: &Vector2) -> MapTag {
    // A : 第一象限 x: - y: + 包含原点与y轴正方向
    // B : 第二象限 x: + y: +,0 包含x轴正方向
    // C : 第三象限 x: - y: -,0  包含x轴负方向
    // D : 第四象限 x: +,0 y: - 包含y轴负方向
    let field_id = get_field_id_from_len_or_pos((pos.x, pos.y));

    // 这里取整,其实就决定了在边线上的分配
    let x_len_point = pos.x / area_len as f32;
    let y_len_point = pos.y / area_len as f32;
    // x_len,y_len 为point向上取整(绝对值向上取值,然后加上原本的符号)
    // println!("x_len_point:{},y_len_point:{}", x_len_point, y_len_point);
    let x_len = if x_len_point >= 0.0 {
        x_len_point.abs().ceil() as i32
    } else {
        -(x_len_point.abs().ceil() as i32)
    };

    let y_len = if y_len_point >= 0.0 {
        y_len_point.abs().ceil() as i32
    } else {
        -(y_len_point.abs().ceil() as i32)
    };

    // 分配的地图区域,不包含另一个区域的顶点(所以,(0,0)是特殊的区域,原本不属于任何一个区域,但是为了方便,我们把它分配到A(1,1)区域))
    let x_len = if x_len == 0 { 1 } else { x_len };
    let y_len = if y_len == 0 { 1 } else { y_len };

    //超出边界的情况
    let x_len = if x_len > 5 { 5 } else { x_len };
    let y_len = if y_len > 5 { 5 } else { y_len };

    MapTag {
        field_id,
        x_len,
        y_len,
    }
}
#[allow(clippy::nonminimal_bool)]
pub fn get_field_id_from_len_or_pos((x, y): (f32, f32)) -> char {
    let field_id = if (0.0 == x && 0.0 == y) || (x == 0.0 && y > 0.0) || (x < 0.0 && y > 0.0) {
        // a区域条件
        'A'
    } else if (x > 0.0 && y > 0.0) || x > 0.0 && y == 0.0 {
        //b 区域条件
        'B'
    } else if (x < 0.0 && y < 0.0) || (x < 0.0 && y == 0.0) {
        //c 区域条件
        'C'
    } else if (x > 0.0 && y < 0.0) || (x == 0.0 && y < 0.0) {
        'D'
    } else {
        // bug 正常情况不会出现
        'E'
    };
    field_id
}

