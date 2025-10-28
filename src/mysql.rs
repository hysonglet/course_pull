use log::{debug, error};
// use calamine::{DataType, Reader, Xlsx, open_workbook};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

use sqlx::{MySql, Pool};

// 定义课程信息结构体
#[derive(Debug)]
pub struct Course {
    pub name: String,
    pub class: String,
    pub teacher: String,
    pub start_week: u32,
    pub end_week: u32,
    pub week_type: WeekType,
    pub location: String,
    pub week: usize,
    pub index: usize,
}

impl fmt::Display for Course {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "课程名称: {}, 教室: {}, 教师: {}, 周次: {}-{}, 周类型: {:?}, 地点: {}, 周数: {}, 课序号: {}",
            self.name,
            self.class,
            self.teacher,
            self.start_week,
            self.end_week,
            self.week_type,
            self.location,
            self.week,
            self.index
        )
    }
}

/// 周范围类型枚举
#[derive(Debug, PartialEq, Eq)]
pub enum WeekType {
    Single,   // 单周
    Double,   // 双周
    Full,     // 全周（连续范围）
    Multiple, // 多个不连续的周
}

/// 周范围结构体
#[derive(Debug, PartialEq)]
pub struct WeekRange {
    pub start_week: u32,
    pub end_week: u32,
    pub week_type: WeekType,
    pub all_weeks: BTreeSet<u32>, // 包含所有周数的集合
}
/// 确定周范围的类型（单周、双周、全周或多个不连续周）
pub fn determine_week_type(weeks: &BTreeSet<u32>, start: u32, end: u32) -> WeekType {
    // 单周情况
    if weeks.len() == 1 {
        return WeekType::Full;
    }

    // 检查是否是连续的全周
    let is_full_range = (start..=end).all(|w| weeks.contains(&w));
    if is_full_range {
        return WeekType::Full;
    }

    // 检查是否是双周（偶数周）
    let all_even = weeks.iter().all(|&w| w % 2 == 0);
    if all_even && weeks.len() == ((end - start) / 2 + 1) as usize {
        return WeekType::Double;
    }

    // 检查是否是单周（奇数周）
    let all_odd = weeks.iter().all(|&w| w % 2 == 1);
    if all_odd && weeks.len() == ((end - start) / 2 + 1) as usize {
        return WeekType::Single;
    }

    // 其他情况视为多个不连续周
    WeekType::Multiple
}

/// 解析周数据字符串，提取所有周数
pub fn parse_weeks(input: &str) -> Result<BTreeSet<u32>, Box<dyn Error>> {
    let mut weeks = BTreeSet::new();

    // 去除括号
    let content = input.trim_start_matches('(').trim_end_matches(')');

    // println!("content: {}", content);

    // 按逗号分割各个部分
    for part in content.split(',') {
        let part = part.trim();

        // 检查是否包含范围符号 "-"
        if part.contains('-') {
            let range_parts: Vec<&str> = part.split('-').collect();
            if range_parts.len() != 2 {
                return Err(format!("无效的范围格式: {}", part).into());
            }

            // 解析起始周和结束周
            let start_str = range_parts[0].trim();
            let end_str = range_parts[1].trim_end_matches("周").trim();

            let start_week: u32 = start_str.parse()?;
            let end_week: u32 = end_str.parse()?;

            // 添加范围内的所有周
            for week in start_week..=end_week {
                weeks.insert(week);
            }
        } else {
            // 处理单周情况
            let week_str = part.trim_end_matches("周").trim();
            let week: u32 = week_str.parse()?;
            weeks.insert(week);
        }
    }

    Ok(weeks)
}

impl WeekRange {
    /// 从周数集合创建WeekRange实例
    pub fn from_weeks(weeks: BTreeSet<u32>) -> Result<Self, Box<dyn Error>> {
        if weeks.is_empty() {
            return Err("没有周数据".into());
        }

        let start_week = *weeks.iter().next().unwrap();
        let end_week = *weeks.iter().next_back().unwrap();
        let week_type = determine_week_type(&weeks, start_week, end_week);

        Ok(Self {
            start_week,
            end_week,
            week_type,
            all_weeks: weeks,
        })
    }
}

pub async fn insert_course(pool: &Pool<MySql>, course: &Course) -> Result<(), Box<dyn Error>> {
    let mysql = format!(
        "INSERT INTO class_schedules 
                            (course, class_id, teacher, 
                            week_type, week_day, start_period, duration, classroom,
                            start_week, end_week)
                        VALUES
                            (
                                '{}', 
                                (SELECT class_id FROM classes WHERE class_name = '{}'),
                                '{}', 
                                '{}', {}, {}, {},'{}', {}, {}
                            );",
        &course.name,
        &course.class,
        &course.teacher,
        match course.week_type {
            WeekType::Single => "single",
            WeekType::Double => "double",
            WeekType::Full => "both",
            WeekType::Multiple => panic!("多周课程不支持"),
        },
        course.week + 1,
        course.index as i32 * 2 + 1,
        2,
        &course.location,
        course.start_week,
        course.end_week,
    );

    debug!("mysql: {}", mysql);

    let rst = sqlx::query(&mysql).execute(pool).await;

    match rst {
        Ok(_) => {
            debug!("插入成功: {}", course);
        }
        Err(e) => {
            error!("插入失败: {}", e);
            panic!("{}", e);
        }
    }

    Ok(())
}

pub async fn get_mysql_poll(url: &str) -> Result<Pool<MySql>, Box<dyn Error>> {
    let pool = Pool::<MySql>::connect(url).await?;
    Ok(pool)
}
