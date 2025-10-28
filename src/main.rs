use crate::mysql::Course;
use clap::Parser;
use clipboard::{ClipboardContext, ClipboardProvider};
use log::{debug, info, warn};
use reqwest::Client;
use scraper::{Html, Selector};
use std::{env, error::Error};

mod mysql;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(short = 'c', long = "cookie", help = "The cookie to use")]
    cookie: Option<String>,

    #[arg(
        short = 'p',
        long = "clipboard-cookie",
        default_value = "false",
        help = "The url to use"
    )]
    clipboard_cookie: bool,

    #[arg(
        short = 'e',
        long = "env",
        default_value = "dev",
        help = "The env to use, [prod|dev]"
    )]
    env: String,

    #[arg(
        short = 'v',
        long = "mysql-version",
        default_value = "1.1.1",
        help = "The mysql version to use"
    )]
    mysql_version: String,
}

// 教务系统请求地址
const URL: &str = "http://jwxt.sygyzyedu.com:8081/jsxsd/kbcx/kbxx_xzb_ifr";
// 教务系统请求班级课表的header
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/113.0.0.0 Safari/537.36";

async fn get_course_info(cookie: &str) -> Result<Vec<Course>, Box<dyn Error>> {
    let client = Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(20))
        .build()?;

    // 请求课表信息
    let response = client.get(URL).header("Cookie", cookie).send().await?;

    info!("status:  {}", &response.status());

    // debug!("Response: {:?}", response.text().await?);
    if response.status().is_success() {
        let html = response.text().await?;
        // 解析课表信息
        Ok(parse_classes_course_info(html.as_str()).await?)
    } else {
        warn!("请求失败: {}", response.status());
        return Err("请求失败".into());
    }
}

async fn parse_classes_course_info(html: &str) -> Result<Vec<Course>, Box<dyn Error>> {
    let document = Html::parse_document(html);

    let title = document
        .select(&Selector::parse("title").unwrap())
        .next()
        .map(|el| el.text().collect::<String>())
        .unwrap_or_else(|| "No title".to_string());

    info!("Title: {:?}", title);

    if title.contains("登录") {
        return Err("登录已过期".into());
    }

    let table_selector = Selector::parse("#timetable").unwrap();
    let tr_selector = Selector::parse("tr").unwrap();
    let td_selector = Selector::parse("td").unwrap();
    let th_selector = Selector::parse("th").unwrap();

    // 获取 table 标签

    let mut class_course: Vec<mysql::Course> = Vec::new();

    if let Some(table) = document.select(&table_selector).next() {
        // let th = table.select(&th_selector).next().unwrap();
        for (row_idx, row) in table.select(&tr_selector).enumerate() {
            // 区分表头行（第一行）和数据行
            let (cell_selector, _row_type) = if row_idx == 0 {
                (&th_selector, "表头")
            } else if row_idx == 1 {
                (&td_selector, "时间")
            } else {
                (&td_selector, "数据")
            };

            // 前面两行是标题，因此不需要处理
            if row_idx < 2 {
                continue;
            }

            let mut class_name = String::new();
            let row = row.select(&cell_selector);
            let count = row.clone().count();
            for (index, td) in row.enumerate() {
                let mut text = td.text().collect::<String>().trim().to_string();

                if index == 0 {
                    class_name = text.clone();
                } else {
                    text = text.replace(&class_name, "");
                }

                // 不为空， 且不是第一个，不是最后一个列
                if !text.is_empty() && index != 0 && index != count - 1 {
                    let cells: Vec<String> = text.split("\n").map(|s| s.to_string()).collect();
                    // info!("{} {:?}", index, cells);

                    let slot = index - 1;
                    let week = slot / 5;
                    let timeslot = slot % 5;

                    cells.chunks(5).for_each(|chunk| {
                        debug!("chunk: {:?}", chunk);
                        let name = chunk.get(0).unwrap_or(&"None".to_string()).to_string();
                        let class = class_name.clone();
                        let teacher = chunk.get(1).unwrap_or(&"None".to_string()).to_string();
                        let weeks = chunk.get(2).unwrap_or(&"None".to_string()).to_string();
                        let location = chunk.get(3).unwrap_or(&"Unknown".to_string()).to_string();

                        let weeks =
                            mysql::WeekRange::from_weeks(mysql::parse_weeks(&weeks).unwrap())
                                .unwrap();

                        // 根据周的范围重新设置课程节点
                        match weeks.week_type {
                            mysql::WeekType::Multiple => {
                                for w in weeks.all_weeks {
                                    let course = mysql::Course {
                                        name: name.clone(),
                                        class: class.clone(),
                                        teacher: teacher.clone(),
                                        start_week: w,
                                        end_week: w,
                                        week_type: mysql::WeekType::Full,
                                        location: location.clone(),
                                        week,
                                        index: timeslot,
                                    };
                                    class_course.push(course);
                                }
                            }

                            _ => {
                                let course = mysql::Course {
                                    name,
                                    class,
                                    teacher,
                                    start_week: weeks.start_week,
                                    end_week: weeks.end_week,
                                    week_type: weeks.week_type,
                                    location,
                                    week,
                                    index: timeslot,
                                };
                                class_course.push(course)
                            }
                        };
                    });
                }
            }
        }
    }
    Ok(class_course)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    env_logger::init();

    let cookie = if let Some(cookie) = cli.cookie {
        // 从命令行参数获取cookie
        cookie
    } else if cli.clipboard_cookie {
        // 从剪贴板获取cookie
        let mut ctx: ClipboardContext = ClipboardProvider::new()?;
        let cookie = ctx.get_contents()?.trim().to_string();
        cookie
    } else {
        return Err("No cookie provided".into());
    };

    info!("Cookie: {}", cookie);

    let class_course = get_course_info(&cookie).await?;

    // 生产服务器mysql地址
    let mysql_url_prod = env::var("MYSQL_URL_PROD").map_err(|e| e.to_string())?;
    // 开发服务器mysql地址
    let mysql_url_dev = env::var("MYSQL_URL_DEV").map_err(|e| e.to_string())?;

    let mysql_url = if cli.env == "prod" {
        &mysql_url_prod
    } else {
        &mysql_url_dev
    };

    info!("env: {}", cli.env);
    info!("mysql_url: {}", mysql_url);

    let conn = mysql::get_mysql_poll(&mysql_url).await?;

    // 清空课程表，避免重复
    let _rst = sqlx::query("TRUNCATE TABLE class_schedules")
        .execute(&conn)
        .await;

    let mut fail_count = 0;
    // 依次插入所有新的课表
    for (index, course) in class_course.iter().enumerate() {
        info!("{}/{}", index + 1, class_course.len());
        let rst = mysql::insert_course(&conn, &course).await;
        if rst.is_err() {
            fail_count += 1;
        }
    }

    // 插入新的版本号
    let _rst = sqlx::query(
        format!(
            "INSERT INTO version(version)
            VALUES 
                ('{}');",
            cli.mysql_version
        )
        .as_str(),
    )
    .execute(&conn)
    .await;

    info!("fail_count: {}, total: {}", fail_count, class_course.len());

    return Ok(());
}
