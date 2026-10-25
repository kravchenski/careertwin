//! The interviewer on a real model. Ignored by default: it needs a running gateway.
//! `LIVE_URL=http://127.0.0.1:3260 LIVE_MODEL=auto cargo test --test live_llm -- --ignored --nocapture`
//!
//! It uses only the fictional demo profile, never your own ABOUT.md.

use std::sync::Arc;
use std::time::Duration;

use careertwin_prep::brief::Brief;
use careertwin_prep::config::LlmSettings;
use careertwin_prep::conversation::{self, Command, Config, Deps, Event, Knobs, State};
use careertwin_prep::llm::HttpLlm;
use careertwin_prep::prompts::Lang;
use careertwin_prep::prompts::interviewer::InterviewerSpec;
use careertwin_prep::secrets::Secret;
use careertwin_prep::session::{Speaker as Who, Track};
use careertwin_prep::speech::{SttHint, Voice};
use careertwin_prep::testing::{FakeSpeaker, FakeStt, FakeTts};

const DEMO: &str = include_str!("fixtures/ABOUT.demo.md");

async fn turn_text(events: &mut tokio::sync::mpsc::UnboundedReceiver<Event>, who: Who) -> String {
    let asked = std::time::Instant::now();
    let mut first = true;
    loop {
        let event = tokio::time::timeout(Duration::from_secs(120), events.recv())
            .await
            .expect("no event in 2 minutes")
            .expect("the call ended");
        match event {
            Event::Caption { .. } if first => {
                first = false;
                println!("          (first sentence after {:.1}s)", asked.elapsed().as_secs_f32());
            }
            Event::Turn { who: w, text, .. } if w == who => return text,
            Event::Error(error) => panic!("error: {error}"),
            _ => {}
        }
    }
}

async fn listening(events: &mut tokio::sync::mpsc::UnboundedReceiver<Event>) {
    loop {
        match tokio::time::timeout(Duration::from_secs(120), events.recv()).await.expect("timeout").expect("ended") {
            Event::State(State::Listening) => return,
            Event::Error(error) => panic!("error: {error}"),
            _ => {}
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a model gateway"]
async fn a_real_model_interviews_like_a_person() {
    let url = std::env::var("LIVE_URL").unwrap_or_else(|_| "http://127.0.0.1:3260".into());
    let model = std::env::var("LIVE_MODEL").unwrap_or_else(|_| "auto".into());
    let track = match std::env::var("LIVE_TRACK").as_deref() {
        Ok("english") => Track::English,
        Ok("interview") => Track::Interview,
        _ => Track::Technical,
    };
    let llm = HttpLlm::new(
        &LlmSettings { url, key: Secret::default(), model: model.clone(), fast_model: String::new() },
        vec![],
    )
    .unwrap();
    let mut spec = InterviewerSpec::new(track);
    spec.minutes = 20;
    let mut brief = Brief { candidate: DEMO.to_owned(), ..Brief::default() };
    brief.set_vacancy("Backend Engineer", "Acme", "We build logistics APIs in Python with FastAPI and PostgreSQL, deployed in Docker. You will own services end to end.");
    let config = Config {
        spec,
        brief,
        knobs: Knobs::default(),
        model,
        headphones: false,
        voice: Voice { name: "alloy".into(), speed: 1.0, instructions: String::new(), language: Lang::En },
        voice_alt: None,
        stt_hint: SttHint::default(),
    };
    let deps = Deps {
        llm: Arc::new(llm),
        stt: FakeStt::new(&[]),
        tts: Some(FakeTts::new()),
        speaker: FakeSpeaker::new(),
        mic: None,
        wake: Arc::new(|| {}),
    };
    let (commands, command_rx) = tokio::sync::mpsc::unbounded_channel();
    let (event_tx, mut events) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(conversation::run(deps, config, command_rx, event_tx));

    let technical = [
        "Hi Daniel, nice to meet you. I work on an order API in Python with FastAPI and PostgreSQL.",
        "Last year the latency doubled after a release. Um, I just added an index.",
        "Я не уверен как сказать... we looked at the slow query log and found the query on customer id.",
        "Honestly I have not used Kubernetes in production, only in a university course.",
    ];
    let english = [
        "Hello! I am fine, thank you. I have worked in Northwind since three years.",
        "I am building API for the orders, and yesterday I fixed one bug what was making the system slow.",
        "How to say... он был в базе данных... the query was not have the index.",
        "I want to improve my speaking because next month I have interview.",
    ];
    let answers = if track == Track::English { english } else { technical };
    let started = std::time::Instant::now();
    let first = turn_text(&mut events, Who::Interviewer).await;
    println!("\n[{:>4.1}s] INTERVIEWER: {first}", started.elapsed().as_secs_f32());
    for answer in answers {
        listening(&mut events).await;
        println!("[{:>4.1}s] YOU:         {answer}", started.elapsed().as_secs_f32());
        let sent = std::time::Instant::now();
        let _ = commands.send(Command::Say(answer.into()));
        let reply = turn_text(&mut events, Who::Interviewer).await;
        println!(
            "[{:>4.1}s] INTERVIEWER: {reply}   (reply took {:.1}s)",
            started.elapsed().as_secs_f32(),
            sent.elapsed().as_secs_f32()
        );
    }
    let _ = commands.send(Command::Finish);
}

/// The tutor on a real model: a terms lesson with a learner who knows one term, is unsure of the
/// next and says "I don't know" once. It prints the whole conversation and what the call
/// understood from the tags.
///
/// `LIVE_URL=http://127.0.0.1:3260 LIVE_MODEL=auto cargo test --test live_llm tutor -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a model gateway"]
async fn a_real_model_tutors_terms_in_your_language_then_in_english() {
    use careertwin_prep::prompts::tutor::{Lesson, Mode};
    use careertwin_prep::syllabus::Term;
    let url = std::env::var("LIVE_URL").unwrap_or_else(|_| "http://127.0.0.1:3260".into());
    let model = std::env::var("LIVE_MODEL").unwrap_or_else(|_| "auto".into());
    let llm = HttpLlm::new(
        &LlmSettings { url, key: Secret::default(), model: model.clone(), fast_model: String::new() },
        vec![],
    )
    .unwrap();
    let mut spec = InterviewerSpec::tutoring(Track::Technical, Mode::Terms, Lang::Ru, 20);
    spec.target = std::env::var("LIVE_TARGET").unwrap_or_default();
    spec.lesson = Some(Lesson {
        topic: "Object-oriented programming".into(),
        terms: vec![
            Term {
                name: "Encapsulation".into(),
                plain: "Объект прячет своё внутреннее устройство и показывает наружу только то, что нужно.".into(),
                analogy: "Кофемашина: нажимаешь кнопку и не лезешь в бойлер.".into(),
                english: "Encapsulation means an object hides its internal state and exposes only what is needed."
                    .into(),
                follow_ups: vec!["Why is it useful?".into()],
                visual: String::new(),
            },
            Term {
                name: "Polymorphism".into(),
                plain: "Один и тот же вызов работает по-разному для разных объектов.".into(),
                analogy: "Кнопка «воспроизвести» в плеере: для музыки, видео и подкаста она делает своё.".into(),
                english:
                    "Polymorphism lets us call the same method on different objects and get behaviour that fits each."
                        .into(),
                follow_ups: vec!["Can you give an example?".into()],
                visual: String::new(),
            },
        ],
    });
    let mut brief = Brief { candidate: DEMO.to_owned(), ..Brief::default() };
    brief.set_vacancy("Backend Engineer", "Acme", "Python, FastAPI and PostgreSQL.");
    let config = Config {
        spec,
        brief,
        knobs: Knobs::default(),
        model,
        headphones: false,
        voice: Voice { name: "alloy".into(), speed: 1.0, instructions: String::new(), language: Lang::En },
        voice_alt: None,
        stt_hint: SttHint::default(),
    };
    let deps = Deps {
        llm: Arc::new(llm),
        stt: FakeStt::new(&[]),
        tts: Some(FakeTts::new()),
        speaker: FakeSpeaker::new(),
        mic: None,
        wake: Arc::new(|| {}),
    };
    let (commands, command_rx) = tokio::sync::mpsc::unbounded_channel();
    let (event_tx, mut events) = tokio::sync::mpsc::unbounded_channel();
    let (tap_tx, mut tap) = tokio::sync::mpsc::unbounded_channel();
    // a relay that prints what the call understood and passes everything on
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            match &event {
                Event::Tag(tag) => println!("          [tag] {tag:?}"),
                Event::TermProgress { name, half, score, .. } => {
                    println!("          [progress] {name} {half:?} {score}")
                }
                Event::Lesson(view) => println!(
                    "          [lesson] term {} of {}: {} ({:?})",
                    view.index + 1,
                    view.total,
                    view.name,
                    view.stage
                ),
                _ => {}
            }
            if tap_tx.send(event).is_err() {
                break;
            }
        }
    });
    tokio::spawn(conversation::run(deps, config, command_rx, event_tx));

    // the first four answers are in Russian: it must stay in Russian until the person asks for English
    let answers = [
        "Это когда мы прячем внутренности объекта и даём доступ только через методы.",
        "Хм, честно, не знаю, что это.",
        "Ну, наверное, когда у разных классов один метод, но они делают разное?",
        "Да, понятно, спасибо.",
        "Давай попрактикуемся на английском.",
        "Encapsulation is when an object hides its data and gives access only by methods. It helps to change the inside without breaking others.",
        "Я не понял, что ты сейчас сказал, объясни по-русски.",
    ];
    let started = std::time::Instant::now();
    let first = turn_text(&mut tap, Who::Interviewer).await;
    println!("\n[{:>4.1}s] TUTOR: {first}", started.elapsed().as_secs_f32());
    for answer in answers {
        listening(&mut tap).await;
        println!("[{:>4.1}s] YOU:   {answer}", started.elapsed().as_secs_f32());
        let _ = commands.send(Command::Say(answer.into()));
        let reply = turn_text(&mut tap, Who::Interviewer).await;
        println!("[{:>4.1}s] TUTOR: {reply}", started.elapsed().as_secs_f32());
        assert!(!reply.contains("[["), "a tag leaked into the words: {reply}");
    }
    let _ = commands.send(Command::Finish);
}

/// A visual explanation on a real model: it prints every scene, what is said and what is shown.
///
/// `LIVE_TOPIC="How a hash map works" cargo test --test live_llm picture -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a model gateway"]
async fn a_real_model_draws_an_explanation_a_beginner_can_follow() {
    let url = std::env::var("LIVE_URL").unwrap_or_else(|_| "http://127.0.0.1:3260".into());
    let model = std::env::var("LIVE_MODEL").unwrap_or_else(|_| "auto".into());
    let topic = std::env::var("LIVE_TOPIC").unwrap_or_else(|_| "How a hash map works".into());
    let llm = HttpLlm::new(
        &LlmSettings { url, key: Secret::default(), model: model.clone(), fast_model: String::new() },
        vec![],
    )
    .unwrap();
    let started = std::time::Instant::now();
    let board = careertwin_prep::ops::storyboard(&llm, &model, &topic, "", Lang::Ru, false).await.expect("a story");
    println!("\n{} ({:.1}s) — check: {}", board.title, started.elapsed().as_secs_f32(), board.check);
    for (number, scene) in board.scenes.iter().enumerate() {
        println!("\n  scene {}: {}", number + 1, scene.say);
        for panel in &scene.panels {
            println!(
                "    [{:?} {}] {} | lines={:?} items={:?} highlight={:?} active={:?} note={:?} left={:?} right={:?}",
                panel.kind,
                panel.id,
                panel.title,
                panel.lines,
                panel.items,
                panel.highlight,
                panel.active,
                panel.note,
                panel.left.lines,
                panel.right.lines
            );
        }
    }
    if let Ok(path) = std::env::var("LIVE_SAVE") {
        std::fs::write(path, serde_json::to_string_pretty(&board).unwrap()).unwrap();
    }
    assert!(board.scenes.len() >= 4, "a story of at least four scenes");
}

/// The coding tutor on a real model: a task is written, then the observer looks at five moments of
/// a session and the test prints what it would say.
///
/// `cargo test --test live_llm coding -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a model gateway"]
async fn a_real_model_sets_a_task_and_watches_without_giving_the_answer() {
    use careertwin_prep::prompts::coding::Level;
    let url = std::env::var("LIVE_URL").unwrap_or_else(|_| "http://127.0.0.1:3260".into());
    let model = std::env::var("LIVE_MODEL").unwrap_or_else(|_| "auto".into());
    let llm = HttpLlm::new(
        &LlmSettings { url, key: Secret::default(), model: model.clone(), fast_model: String::new() },
        vec![],
    )
    .unwrap();
    let mut brief = Brief { candidate: DEMO.to_owned(), ..Brief::default() };
    brief.set_vacancy("Backend Engineer", "Acme", "We build logistics APIs in Python with FastAPI and PostgreSQL.");
    // a fixed task, so the sample code below is about it
    let task = careertwin_prep::prompts::coding::parse_task(
        r#"{"title": "Two sum", "statement": "Return the positions of the two numbers in the list that add up to the target.",
 "starter": "def two_sum(nums, target):\n    # return the two positions", "examples": [{"input": "[2, 7, 11], 9", "output": "[0, 1]"}],
 "outline": "One pass with a dictionary of numbers seen so far, looking up target - n.", "pitfalls": ["using the same element twice", "returning values instead of positions"],
 "hints": ["What do you need to remember about the numbers you have already seen?", "How could you find target - n without looking at every other number?"], "complexity": "O(n) time, O(n) space"}"#,
        "python",
    )
    .unwrap();
    // and a written one, to see that writing a task works
    let started = std::time::Instant::now();
    let request = careertwin_prep::ops::TaskRequest { language: "python", level: Level::WarmUp, topic: "", avoid: &[] };
    let written = careertwin_prep::ops::coding_task(&llm, &model, &brief, Lang::En, request).await.expect("a task");
    println!(
        "\nA WRITTEN TASK ({:.1}s): {} — {}\nstarter: {:?}\n",
        started.elapsed().as_secs_f32(),
        written.title,
        written.statement,
        written.starter
    );

    let moments: [(&str, String, &str); 6] = [
        (
            "on the right track, unfinished",
            "def two_sum(nums, target):\n    seen = {}\n    for i, n in enumerate(nums):\n        ".to_owned(),
            "The session is 3 minutes old.",
        ),
        (
            "a bug: it stores the number before looking for its partner, and returns the value",
            "def two_sum(nums, target):\n    seen = {}\n    for i, n in enumerate(nums):\n        seen[n] = i\n        if target - n in seen:\n            return [target - n, n]\n".to_owned(),
            "The session is 6 minutes old.",
        ),
        (
            "a whole solution pasted",
            "def two_sum(nums, target):\n    seen = {}\n    for i, n in enumerate(nums):\n        if target - n in seen:\n            return [seen[target - n], i]\n        seen[n] = i\n".to_owned(),
            "A block of 210 characters (7 lines) appeared all at once 4 seconds ago; it was not typed.",
        ),
        (
            "nothing happening for a long time",
            task.starter.clone(),
            "Nothing has been typed for 95 seconds.\nThe session is 5 minutes old.",
        ),
        (
            "it looks finished and right",
            "def two_sum(nums, target):\n    seen = {}\n    for i, n in enumerate(nums):\n        if target - n in seen:\n            return [seen[target - n], i]\n        seen[n] = i\n    return []\n".to_owned(),
            "The session is 11 minutes old.",
        ),
        (
            "the window was left for a minute and then a full solution appeared",
            "def two_sum(nums, target):\n    d = dict(zip(nums, range(len(nums))))\n    for i, n in enumerate(nums):\n        j = d.get(target - n)\n        if j is not None and j != i:\n            return [i, j]\n".to_owned(),
            "The window was out of focus for 64 seconds, 6 seconds ago.\nThe session is 9 minutes old.",
        ),
    ];
    for (what, code, signals) in moments {
        let asked = std::time::Instant::now();
        let look = careertwin_prep::ops::Look { code: &code, said: &[], signals, last_words: &[] };
        let seen = careertwin_prep::ops::observe(&llm, &model, &task, look, Lang::En).await.expect("an observation");
        println!(
            "[{:>4.1}s] {what}\n        -> {:?} level {} say: {:?}\n",
            asked.elapsed().as_secs_f32(),
            seen.status,
            seen.level,
            seen.say
        );
    }
}

/// The coding tutor in a call: it introduces the task, listens to an approach, sees a wrong draft,
/// and is told "I don't know". It must ask rather than tell.
///
/// `cargo test --test live_llm conversation -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a model gateway"]
async fn a_real_model_talks_through_a_coding_task_without_handing_over_the_answer() {
    use careertwin_prep::prompts::tutor::Mode;
    let url = std::env::var("LIVE_URL").unwrap_or_else(|_| "http://127.0.0.1:3260".into());
    let model = std::env::var("LIVE_MODEL").unwrap_or_else(|_| "auto".into());
    let llm = HttpLlm::new(
        &LlmSettings { url, key: Secret::default(), model: model.clone(), fast_model: String::new() },
        vec![],
    )
    .unwrap();
    let task = careertwin_prep::prompts::coding::parse_task(
        r#"{"title": "Two sum", "statement": "Return the positions of the two numbers in the list that add up to the target.",
 "starter": "def two_sum(nums, target):\n    # return the two positions", "examples": [{"input": "[2, 7, 11], 9", "output": "[0, 1]"}],
 "outline": "One pass with a dictionary of numbers seen so far, looking up target - n.", "pitfalls": ["using the same element twice", "returning values instead of positions"],
 "hints": ["What do you need to remember about the numbers you have already seen?", "How could you find target - n without looking at every other number?"], "complexity": "O(n) time, O(n) space"}"#,
        "python",
    )
    .unwrap();
    let mut spec = InterviewerSpec::tutoring(Track::Technical, Mode::Coding, Lang::Ru, 30);
    spec.language = Lang::En;
    spec.task = Some(task);
    let mut brief = Brief { candidate: DEMO.to_owned(), ..Brief::default() };
    brief.set_vacancy("Backend Engineer", "Acme", "Python and PostgreSQL.");
    let config = Config {
        spec,
        brief,
        knobs: Knobs::default(),
        model,
        headphones: false,
        voice: Voice { name: "alloy".into(), speed: 1.0, instructions: String::new(), language: Lang::En },
        voice_alt: None,
        stt_hint: SttHint::default(),
    };
    let deps = Deps {
        llm: Arc::new(llm),
        stt: FakeStt::new(&[]),
        tts: Some(FakeTts::new()),
        speaker: FakeSpeaker::new(),
        mic: None,
        wake: Arc::new(|| {}),
    };
    let (commands, command_rx) = tokio::sync::mpsc::unbounded_channel();
    let (event_tx, mut events) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(conversation::run(deps, config, command_rx, event_tx));
    let started = std::time::Instant::now();
    let first = turn_text(&mut events, Who::Interviewer).await;
    println!("\n[{:>4.1}s] TUTOR:  {first}", started.elapsed().as_secs_f32());
    let script: [(&str, &str); 4] = [
        (
            "def two_sum(nums, target):\n    for i in range(len(nums)):\n        for j in range(len(nums)):\n            if nums[i] + nums[j] == target:\n                return [i, j]\n",
            "I would try every pair with two loops, I think.",
        ),
        (
            "def two_sum(nums, target):\n    seen = {}\n    for i, n in enumerate(nums):\n        seen[n] = i\n        if target - n in seen:\n            return [seen[target - n], i]\n",
            "Okay, now I use a dictionary to remember numbers.",
        ),
        (
            "def two_sum(nums, target):\n    seen = {}\n    for i, n in enumerate(nums):\n        seen[n] = i\n",
            "Я не знаю, как дальше найти пару.",
        ),
        (
            "def two_sum(nums, target):\n    seen = {}\n    for i, n in enumerate(nums):\n        if target - n in seen:\n            return [seen[target - n], i]\n        seen[n] = i\n    return []\n",
            "I think I'm done. Can you check it?",
        ),
    ];
    for (code, said) in script {
        listening(&mut events).await;
        let _ = commands.send(Command::Code(code.into()));
        println!("[{:>4.1}s] CODE:\n{code}", started.elapsed().as_secs_f32());
        println!("[{:>4.1}s] PERSON: {said}", started.elapsed().as_secs_f32());
        let _ = commands.send(Command::Say(said.into()));
        let reply = turn_text(&mut events, Who::Interviewer).await;
        println!("[{:>4.1}s] TUTOR:  {reply}", started.elapsed().as_secs_f32());
    }
    let _ = commands.send(Command::Finish);
}

/// The research of a vacancy on a real model: the plan it makes, ranked by the 80/20 rule.
///
/// `cargo test --test live_llm research -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a model gateway"]
async fn a_real_model_researches_a_vacancy_and_ranks_what_will_be_asked() {
    let url = std::env::var("LIVE_URL").unwrap_or_else(|_| "http://127.0.0.1:3260".into());
    let model = std::env::var("LIVE_MODEL").unwrap_or_else(|_| "auto".into());
    let llm = HttpLlm::new(
        &LlmSettings { url, key: Secret::default(), model: model.clone(), fast_model: String::new() },
        vec![],
    )
    .unwrap();
    let mut brief = Brief { candidate: DEMO.to_owned(), ..Brief::default() };
    brief.set_vacancy(
        "AI Native Full Stack Developer",
        "Acme",
        "We are looking for a full stack developer who builds products with LLMs. You will work with TypeScript, React and \
         Node.js on the front end and back end, design APIs, integrate LLM APIs (prompting, tool calling, RAG, evaluation), \
         use PostgreSQL and Redis, write tests and ship with Docker and CI. You use AI coding tools every day and review \
         their output critically.",
    );
    let progress = careertwin_prep::ops::ResearchProgress::default();
    let started = std::time::Instant::now();
    let plan = careertwin_prep::ops::research(&llm, &model, &brief, Lang::Ru, &progress).await.expect("a plan");
    println!("\nPLAN for {} ({:.1}s)\n{}\n", plan.role, started.elapsed().as_secs_f32(), plan.summary);
    for failure in progress.failures.lock().unwrap().iter() {
        println!("  first try failed: {failure}");
    }
    let vital = plan.vital();
    for (index, topic) in plan.topics.iter().enumerate() {
        println!(
            "  {} {:<34} {:?} likely {} effort {} value {:.1} terms {}   {}",
            if vital.contains(&index) { "★" } else { " " },
            topic.name,
            topic.kind,
            topic.likelihood,
            topic.effort,
            topic.value(),
            topic.terms.len(),
            topic.why
        );
    }
    if let Some(topic) = plan.topics.iter().find(|t| !t.terms.is_empty()) {
        println!("\nTERMS of {}:", topic.name);
        for term in &topic.terms {
            println!(
                "  - {}\n      plain:   {}\n      like:    {}\n      english: {}\n      next:    {:?}\n      picture: {}",
                term.name, term.plain, term.analogy, term.english, term.follow_ups, term.visual
            );
        }
    }
    // a topic that was left without terms must be writable when asked for again
    for topic in plan.topics.iter().filter(|t| t.terms.is_empty()).take(3) {
        let asked = std::time::Instant::now();
        match careertwin_prep::ops::topic_terms(&llm, &model, &brief, topic, Lang::Ru).await {
            Ok(terms) => {
                println!("  again: {} -> {} terms ({:.1}s)", topic.name, terms.len(), asked.elapsed().as_secs_f32())
            }
            Err(error) => println!("  again: {} FAILED ({:.1}s): {error}", topic.name, asked.elapsed().as_secs_f32()),
        }
    }
    assert!(plan.topics.len() >= 6);
    assert!(plan.topics.iter().filter(|t| !t.terms.is_empty()).count() >= 3);
}

/// The introduction coach on a real model: it asks for the introduction, coaches one answer and
/// offers the shape, in English, using only facts from the demo profile.
///
/// `cargo test --test live_llm introduction -- --ignored --nocapture`
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a model gateway"]
async fn a_real_model_coaches_an_introduction() {
    use careertwin_prep::prompts::tutor::Mode;
    let url = std::env::var("LIVE_URL").unwrap_or_else(|_| "http://127.0.0.1:3260".into());
    let model = std::env::var("LIVE_MODEL").unwrap_or_else(|_| "auto".into());
    let llm = HttpLlm::new(
        &LlmSettings { url, key: Secret::default(), model: model.clone(), fast_model: String::new() },
        vec![],
    )
    .unwrap();
    let mut spec = InterviewerSpec::tutoring(Track::Interview, Mode::Intro, Lang::Ru, 15);
    spec.language = Lang::En;
    let mut brief = Brief { candidate: DEMO.to_owned(), ..Brief::default() };
    brief.set_vacancy("Backend Engineer", "Acme", "Python and PostgreSQL.");
    let config = Config {
        spec,
        brief,
        knobs: Knobs::default(),
        model,
        headphones: false,
        voice: Voice { name: "alloy".into(), speed: 1.0, instructions: String::new(), language: Lang::En },
        voice_alt: None,
        stt_hint: SttHint::default(),
    };
    let deps = Deps {
        llm: Arc::new(llm),
        stt: FakeStt::new(&[]),
        tts: Some(FakeTts::new()),
        speaker: FakeSpeaker::new(),
        mic: None,
        wake: Arc::new(|| {}),
    };
    let (commands, command_rx) = tokio::sync::mpsc::unbounded_channel();
    let (event_tx, mut events) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(conversation::run(deps, config, command_rx, event_tx));
    let started = std::time::Instant::now();
    let first = turn_text(&mut events, Who::Interviewer).await;
    println!("\n[{:>4.1}s] COACH: {first}", started.elapsed().as_secs_f32());
    for said in [
        "Hi, I am Alex. I work as a backend developer. I did many things, Python, some databases, also Docker. I like to learn new things and I work in a team.",
        "I work at Northwind on the order API in FastAPI, I added an index that fixed a slow query, and I want this job because you use PostgreSQL.",
    ] {
        listening(&mut events).await;
        println!("[{:>4.1}s] YOU:   {said}", started.elapsed().as_secs_f32());
        let _ = commands.send(Command::Say(said.into()));
        let reply = turn_text(&mut events, Who::Interviewer).await;
        println!("[{:>4.1}s] COACH: {reply}", started.elapsed().as_secs_f32());
        assert!(!reply.contains("[["), "a tag leaked: {reply}");
    }
    let _ = commands.send(Command::Finish);
}
