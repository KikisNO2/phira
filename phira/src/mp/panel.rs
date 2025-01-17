use super::mtl;
use crate::{
    client::{Chart, Ptr},
    dir, get_data,
    mp::L10N_LOCAL,
    scene::{Downloading, SongScene, RECORD_ID},
};
use anyhow::{anyhow, Context, Result};
use macroquad::prelude::*;
use phira_mp_client::Client;
use phira_mp_common::RoomState;
use prpr::{
    config::Mods,
    core::Tweenable,
    ext::{semi_black, semi_white, RectExt},
    info::ChartInfo,
    scene::{loading_scene, request_input, return_input, show_error, show_message, take_input, take_loaded_scene, GameMode, NextScene},
    task::Task,
    time::TimeManager,
    ui::{DRectButton, DrawText},
    ui::{Scroll, Ui},
};
use smallvec::SmallVec;
use std::{
    fs::File,
    path::Path,
    sync::{atomic::Ordering, Arc},
};
use tokio::net::TcpStream;
use uuid::Uuid;

const ENTER_TRANSIT: f32 = 0.5;
const WIDTH: f32 = 1.6;

fn screen_size() -> (u32, u32) {
    (screen_width() as u32, screen_height() as u32)
}

struct Message {
    content: String,
    y: f32,
    bottom: f32,
    color: Color,
}

impl Message {
    pub fn text<'a, 's, 'ui>(&'s self, ui: &'ui mut Ui<'a>, mw: f32) -> DrawText<'a, 's, 'ui> {
        ui.text(&self.content)
            .pos(0., self.y)
            .size(0.5)
            .color(self.color)
            .max_width(mw)
            .multiline()
    }
}

pub struct MPPanel {
    client: Option<Arc<Client>>,

    side_enter_time: f32,

    msg_scroll: Scroll,
    msgs: Vec<Message>,
    msgs_dirty_from: usize,
    last_screen_size: (u32, u32),

    connect_btn: DRectButton,
    connect_task: Option<Task<Result<Client>>>,

    create_room_btn: DRectButton,
    create_room_task: Option<Task<Result<()>>>,
    join_room_btn: DRectButton,
    join_room_task: Option<Task<Result<()>>>,
    leave_room_btn: DRectButton,

    disconnect_btn: DRectButton,

    request_start_btn: DRectButton,

    ready_btn: DRectButton,
    cancel_ready_btn: DRectButton,

    chat_text: String,
    chat_btn: DRectButton,
    chat_send_btn: DRectButton,
    chat_task: Option<Task<Result<()>>>,

    download_task: Option<Task<Result<Arc<Chart>>>>,
    downloading: Option<Downloading>,
    // true for request_start, false for ready
    download_next: bool,

    chart_id: Option<i32>,
    game_start_consumed: bool,
    need_upload: bool,

    next_scene: Option<NextScene>,

    task: Option<Task<Result<()>>>,
}

impl MPPanel {
    pub fn new() -> Self {
        Self {
            client: None,

            side_enter_time: f32::INFINITY,

            msg_scroll: Scroll::new(),
            msgs: Vec::new(),
            msgs_dirty_from: 0,
            last_screen_size: screen_size(),

            connect_btn: DRectButton::new(),
            connect_task: None,

            create_room_btn: DRectButton::new(),
            create_room_task: None,
            join_room_btn: DRectButton::new(),
            join_room_task: None,
            leave_room_btn: DRectButton::new(),

            disconnect_btn: DRectButton::new(),

            request_start_btn: DRectButton::new(),

            ready_btn: DRectButton::new(),
            cancel_ready_btn: DRectButton::new(),

            chat_text: String::new(),
            chat_btn: DRectButton::new().with_delta(-0.002),
            chat_send_btn: DRectButton::new(),
            chat_task: None,

            download_task: None,
            downloading: None,
            download_next: false,

            chart_id: None,
            game_start_consumed: false,
            need_upload: false,

            next_scene: None,

            task: None,
        }
    }

    fn clone_client(&self) -> Arc<Client> {
        Arc::clone(self.client.as_ref().unwrap())
    }

    fn has_task(&self) -> bool {
        self.connect_task.is_some()
            || self.create_room_task.is_some()
            || self.chat_task.is_some()
            || self.download_task.is_some()
            || self.task.is_some()
            || loading_scene()
    }

    fn connect(&mut self) {
        let Some(token) = get_data().tokens.as_ref().map(|it| it.0.clone()) else {
            show_message(mtl!("connect-must-login")).error();
            return;
        };
        let addr = get_data().config.mp_address.clone();
        self.connect_task = Some(Task::new(async move {
            let client = Client::new(TcpStream::connect(addr).await?).await?;
            client.authorize(token).await.with_context(|| anyhow!(mtl!("connect-authorize-failed")))?;
            Ok(client)
        }));
    }

    fn create_room(&mut self) {
        let client = self.clone_client();
        self.create_room_task = Some(Task::new(async move {
            client.create_room().await?;
            Ok(())
        }));
    }

    pub fn select_chart(&mut self, id: i32) {
        let client = self.clone_client();
        if !client.blocking_is_host().unwrap() {
            show_message(mtl!("select-chart-host-only")).error();
            return;
        }
        self.task = Some(Task::new(async move {
            client.select_chart(id).await.with_context(|| mtl!("select-chart-failed"))?;
            Ok(())
        }));
    }

    fn request_start(&mut self) {
        if matches!(self.client.as_ref().unwrap().blocking_room_state().unwrap(), RoomState::SelectChart(None)) {
            show_message(mtl!("request-start-no-chart")).error();
            return;
        }
        self.check_download(true);
    }

    fn check_download(&mut self, next: bool) {
        let id = self.chart_id.unwrap();
        self.download_next = next;
        self.download_task = Some(Task::new(async move { Ptr::new(id).fetch().await }));
    }

    fn post_download(&mut self) {
        let client = self.clone_client();
        if self.download_next {
            self.task = Some(Task::new(async move {
                client.request_start().await.with_context(|| mtl!("request-start-failed"))?;
                Ok(())
            }));
        } else {
            self.task = Some(Task::new(async move {
                client.ready().await.with_context(|| mtl!("ready-failed"))?;
                Ok(())
            }));
        }
    }
}

impl MPPanel {
    #[inline]
    pub fn active(&self) -> bool {
        self.client.is_some()
    }

    #[inline]
    pub fn show(&mut self, rt: f32) {
        self.side_enter_time = rt;
    }

    pub fn touch(&mut self, tm: &mut TimeManager, touch: &Touch) -> bool {
        let t = tm.now() as f32;
        if self.side_enter_time.is_infinite() {
            return false;
        }
        if !(self.side_enter_time > 0. && tm.real_time() as f32 > self.side_enter_time + ENTER_TRANSIT) {
            return true;
        }
        if self.has_task() {
            return true;
        }
        if let Some(dl) = &mut self.downloading {
            if dl.touch(touch, t) {
                self.downloading = None;
                return true;
            }
        }
        if touch.position.x + 1. > WIDTH {
            self.side_enter_time = -tm.real_time() as f32;
            return true;
        }
        if self.client.is_none() && self.connect_btn.touch(touch, t) {
            self.connect();
            return true;
        }
        if let Some(client) = &self.client {
            if self.msg_scroll.touch(touch, t) {
                return true;
            }
            if let Some(room_state) = client.blocking_room_state() {
                if self.chat_btn.touch(touch, t) {
                    request_input("chat", &self.chat_text);
                    return true;
                }
                if self.chat_send_btn.touch(touch, t) {
                    if self.chat_text.is_empty() {
                        show_message(mtl!("chat-empty")).error();
                    } else {
                        let client = Arc::clone(client);
                        let text = self.chat_text.clone();
                        self.chat_task = Some(Task::new(async move { client.chat(text).await }));
                    }
                    return true;
                }
                let is_host = client.blocking_is_host().unwrap();
                match room_state {
                    RoomState::SelectChart(_) => {
                        if is_host && self.request_start_btn.touch(touch, t) {
                            self.request_start();
                            return true;
                        }
                        if self.leave_room_btn.touch(touch, t) {
                            let client = self.clone_client();
                            self.task = Some(Task::new(async move { client.leave_room().await }));
                            return true;
                        }
                    }
                    RoomState::WaitingForReady => {
                        if client.blocking_is_ready().unwrap() {
                            if self.cancel_ready_btn.touch(touch, t) {
                                let client = self.clone_client();
                                self.task = Some(Task::new(async move { client.cancel_ready().await }));
                                return true;
                            }
                        } else if self.ready_btn.touch(touch, t) {
                            self.check_download(false);
                            return true;
                        }
                    }
                    _ => {}
                }
            } else {
                if self.create_room_btn.touch(touch, t) {
                    self.create_room();
                    return true;
                }
                if self.join_room_btn.touch(touch, t) {
                    request_input("join_room", "");
                    return true;
                }
                if self.disconnect_btn.touch(touch, t) {
                    self.client = None;
                    self.msgs.clear();
                    self.msgs_dirty_from = 0;
                    return true;
                }
            }
        }
        true
    }

    pub fn update(&mut self, tm: &mut TimeManager) -> Result<()> {
        let t = tm.now() as f32;
        if self.side_enter_time < 0. && -tm.real_time() as f32 + ENTER_TRANSIT < self.side_enter_time {
            self.side_enter_time = f32::INFINITY;
        }
        let new_size = screen_size();
        if self.last_screen_size != new_size {
            self.last_screen_size = new_size;
            self.msgs_dirty_from = 0;
        }
        self.msg_scroll.update(t);
        if let Some(client) = &self.client {
            self.msgs.extend(client.blocking_take_messages().into_iter().map(|msg| {
                use phira_mp_common::Message as M;
                match msg {
                    M::Chat { user, content, .. } => Message {
                        content: format!("{user}：{content}"),
                        y: 0.,
                        bottom: 0.,
                        color: WHITE,
                    },
                    _ => {
                        let content = match msg {
                            M::Chat { .. } => unreachable!(),
                            M::CreateRoom { user } => {
                                mtl!("msg-create-room", "user" => user)
                            }
                            M::JoinRoom { user } => {
                                mtl!("msg-join-room", "user" => user)
                            }
                            M::LeaveRoom { user } => {
                                mtl!("msg-leave-room", "user" => user)
                            }
                            M::NewHost { user } => {
                                mtl!("msg-new-host", "user" => user)
                            }
                            M::SelectChart { user, name, id } => {
                                mtl!("msg-select-chart", "user" => user, "chart" => name, "id" => id)
                            }
                            M::GameStart { user } => {
                                mtl!("msg-game-start", "user" => user)
                            }
                            M::Ready { user } => {
                                mtl!("msg-ready", "user" => user)
                            }
                            M::CancelReady { user } => {
                                mtl!("msg-cancel-ready", "user" => user)
                            }
                            M::CancelGame { user } => {
                                mtl!("msg-cancel-game", "user" => user)
                            }
                            M::StartPlaying => mtl!("msg-start-playing").into_owned(),
                            M::Played { user, score, accuracy, full_combo } => {
                                mtl!("msg-played", "user" => user, "score" => format!("{score:07}"), "accuracy" => format!("{:.2}%", accuracy * 100.), "full-combo" => full_combo.to_string())
                            }
                            M::GameEnd => mtl!("msg-game-end").into_owned(),
                        };
                        Message {
                            content,
                            y: 0.,
                            bottom: 0.,
                            color: semi_white(0.7),
                        }
                    }
                }
            }));
            let state = client.blocking_room_state();
            if matches!(state, Some(RoomState::Playing)) {
                if !self.game_start_consumed {
                    self.game_start_consumed = true;
                    let id = self.chart_id.unwrap();
                    RECORD_ID.store(-1, Ordering::Relaxed);
                    self.need_upload = true;
                    SongScene::global_launch(Some(id), &format!("download/{id}"), Mods::default(), GameMode::Normal)?;
                }
            } else {
                self.game_start_consumed = false;
            }
            if let Some(RoomState::SelectChart(chart)) = state {
                self.chart_id = chart;
            }
        }
        if let Some(task) = &mut self.connect_task {
            if let Some(res) = task.take() {
                match res {
                    Ok(client) => {
                        show_message(mtl!("connect-success")).ok();
                        self.client = Some(client.into());
                    }
                    Err(err) => {
                        show_error(err.context(mtl!("connect-failed")));
                    }
                }
                self.connect_task = None;
            }
        }
        if let Some(task) = &mut self.create_room_task {
            if let Some(res) = task.take() {
                match res {
                    Ok(_) => {
                        show_message(mtl!("create-room-success")).ok();
                    }
                    Err(err) => {
                        show_error(err.context(mtl!("create-room-failed")));
                    }
                }
                self.create_room_task = None;
            }
        }
        if let Some(task) = &mut self.download_task {
            if let Some(res) = task.take() {
                match res {
                    Ok(entity) => {
                        let path = format!("download/{}", entity.id);
                        let info_path = format!("{}/{path}/info.yml", dir::charts()?);
                        let should_download = if Path::new(&info_path).exists() {
                            let local_info: ChartInfo = serde_yaml::from_reader(File::open(info_path)?)?;
                            local_info
                                .updated
                                .map_or(entity.updated != entity.created, |local_updated| local_updated != entity.updated)
                        } else {
                            true
                        };
                        if should_download {
                            let info = entity.to_info();
                            self.downloading = Some(SongScene::global_start_download(info, Chart::clone(&entity), {
                                if Path::new(&format!("{}/{path}", dir::charts()?)).exists() {
                                    Some(path)
                                } else {
                                    None
                                }
                            })?);
                        } else {
                            self.post_download();
                        }
                    }
                    Err(err) => {
                        show_error(err.context(mtl!("download-failed")));
                    }
                }
                self.download_task = None;
            }
        }
        if let Some(dl) = &mut self.downloading {
            if let Some(res) = dl.check()? {
                if res {
                    self.post_download();
                }
                self.downloading = None;
            }
        }
        if let Some(task) = &mut self.chat_task {
            if let Some(res) = task.take() {
                match res {
                    Ok(_) => {
                        show_message(mtl!("chat-sent")).ok();
                        self.chat_text.clear();
                    }
                    Err(err) => {
                        show_error(err.context(mtl!("chat-send-failaed")));
                    }
                }
                self.chat_task = None;
            }
        }
        if let Some(task) = &mut self.task {
            if let Some(res) = task.take() {
                if let Err(err) = res {
                    show_error(err);
                }
                self.task = None;
            }
        }
        if let Some((id, text)) = take_input() {
            match id.as_str() {
                "chat" => {
                    self.chat_text = text;
                }
                "join_room" => {
                    let client = self.clone_client();
                    if let Ok(uuid) = Uuid::parse_str(&text) {
                        self.join_room_task = Some(Task::new(async move {
                            client.join_room(uuid).await?;
                            Ok(())
                        }));
                    } else {
                        show_message(mtl!("join-room-invalid-id")).error();
                    }
                }
                _ => return_input(id, text),
            }
        }
        if let Some(res) = take_loaded_scene() {
            match res {
                Err(err) => {
                    show_error(err);
                }
                Ok(scene) => self.next_scene = Some(scene),
            }
        }
        if self.need_upload {
            let id = RECORD_ID.load(Ordering::Relaxed);
            if id != -1 {
                let client = self.clone_client();
                self.task = Some(Task::new(async move { client.played(id).await }));
                self.need_upload = false;
            }
        }
        Ok(())
    }

    pub fn render(&mut self, tm: &mut TimeManager, ui: &mut Ui) {
        let rt = tm.real_time() as f32;
        let t = tm.now() as f32;
        if self.side_enter_time.is_finite() {
            let p = ((rt - self.side_enter_time.abs()) / ENTER_TRANSIT).min(1.);
            let p = 1. - (1. - p).powi(3);
            let p = if self.side_enter_time < 0. { 1. - p } else { p };
            ui.fill_rect(ui.screen_rect(), semi_black(p * 0.6));
            let w = WIDTH;
            let rt = f32::tween(&-1., &(w - 1.), p);
            ui.scope(|ui| {
                ui.dx(rt - w);
                ui.dy(-ui.top);
                let h = ui.top * 2.;
                let r = Rect::new(0., 0., w, h).feather(-0.02);
                ui.fill_path(&r.rounded(0.02), ui.background());
                if let Some(id) = self.client.as_ref().and_then(|it| it.blocking_room_id()) {
                    ui.text(mtl!("room-id", "id" => id.to_string()))
                        .pos(r.right() - 0.02, r.y + 0.02)
                        .anchor(1., 0.)
                        .size(0.44)
                        .color(semi_white(0.4))
                        .draw();
                }
                let tr = ui.text(mtl!("multiplayer")).pos(0.05, 0.05).draw();
                let r = Rect::new(r.x, tr.bottom(), r.w, r.bottom() - tr.bottom()).feather(-0.02);
                if self.client.is_none() {
                    let ct = r.center();
                    self.connect_btn
                        .render_text(ui, Rect::new(ct.x, ct.y, 0., 0.).nonuniform_feather(0.14, 0.06), t, 1., mtl!("connect"), 0.5, true);
                } else {
                    self.render_main(tm, ui, r);
                }
            });
        }
        if let Some(dl) = &mut self.downloading {
            dl.render(ui, t);
        }
        if self.has_task() {
            ui.full_loading_simple(t);
        }
    }

    fn render_main(&mut self, tm: &mut TimeManager, ui: &mut Ui, r: Rect) {
        let t = tm.now() as f32;
        let client = self.client.as_ref().unwrap();
        let mr = Rect::new(r.x, r.y, r.w * 0.8, r.h - 0.11);
        ui.fill_path(&mr.rounded(0.01), semi_black(0.4));
        ui.scope(|ui| {
            let mut mr = mr.feather(-0.015);
            mr.y -= 0.015;
            mr.h += 0.015;
            ui.dx(mr.x);
            ui.dy(mr.y);
            let mut y = if self.msgs_dirty_from == 0 {
                0.
            } else {
                self.msgs.get(self.msgs_dirty_from - 1).map_or(0., |it| it.bottom)
            };
            for msg in &mut self.msgs[self.msgs_dirty_from..] {
                msg.y = y + 0.02;
                msg.bottom = msg.text(ui, mr.w).measure().bottom();
                y = msg.bottom;
            }
            self.msgs_dirty_from = self.msgs.len();
            self.msg_scroll.size((mr.w, mr.h));
            let offset = self.msg_scroll.y_scroller.offset;
            self.msg_scroll.render(ui, |ui| {
                for msg in &self.msgs {
                    if msg.bottom < offset {
                        continue;
                    }
                    if msg.y > offset + mr.h {
                        break;
                    }
                    msg.text(ui, mr.w).draw();
                }
                (mr.w, self.msgs.last().map(|it| it.bottom).unwrap_or_default() + 0.03)
            });
        });

        let lw = 0.16;
        let h = 0.09;
        let br = Rect::new(r.x, r.bottom() - h, mr.w - lw - 0.02, h);
        self.chat_btn.render_input(ui, br, t, 1., &self.chat_text, mtl!("chat-placeholder"), 0.5);
        let br = Rect::new(mr.right() - lw, br.y, lw, br.h);
        self.chat_send_btn.render_text(ui, br, t, 1., mtl!("chat-send"), 0.5, true);

        let mut br = Rect::new(mr.right() + 0.02, mr.y, r.right() - mr.right() - 0.02, 0.1);
        let mut btns = SmallVec::<[(&mut DRectButton, &'static str); 4]>::new();
        if let Some(room_state) = client.blocking_room_state() {
            match room_state {
                RoomState::SelectChart(_) => {
                    if client.blocking_is_host().unwrap() {
                        btns.push((&mut self.request_start_btn, "request-start"));
                    }
                    btns.push((&mut self.leave_room_btn, "leave-room"));
                }
                RoomState::WaitingForReady => {
                    if client.blocking_is_ready().unwrap() {
                        btns.push((&mut self.cancel_ready_btn, "cancel-ready"));
                    } else {
                        btns.push((&mut self.ready_btn, "ready"));
                    }
                }
                _ => {}
            }
        } else {
            btns.push((&mut self.create_room_btn, "create-room"));
            btns.push((&mut self.join_room_btn, "join-room"));
            btns.push((&mut self.disconnect_btn, "disconnect"));
        }
        for (btn, text) in btns {
            btn.render_text(ui, br, t, 1., mtl!(text), 0.5, true);
            br.y += br.h + 0.02;
        }
    }

    #[inline]
    pub fn next_scene(&mut self) -> Option<NextScene> {
        self.next_scene.take()
    }
}
