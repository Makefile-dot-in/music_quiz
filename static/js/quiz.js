const get_questions = () => fetch(`/artist/${ARTIST_ID}/questions.json`)
      .then(res => {
        if (!res.ok)
          throw new Error("An internal error occurred.");
        else
          return res;
      })
      .then(res => res.json());

let questions = get_questions();

function wait_event(el, evtyp) {
  return new Promise(resolve => el.addEventListener(evtyp, resolve, { once: true }));
}

class ViewsElement extends HTMLElement {
  constructor() {
    super();
  }

  async hide() {
    const promises = [];
    for (const child of this.children) {
      const promise = wait_event(child.animate({opacity: [1, 0]}, 500), "finish")
            .then(() => child.classList.add("hidden"));
      promises.push(promise);
    }

    await Promise.all(promises);
  }

  async show(sel) {
    const switch_to_el = this.querySelector(sel);
    switch_to_el.classList.remove("hidden");
    await wait_event(switch_to_el.animate({opacity: [0, 1]}, 500), "finish");
  }

  async switch(sel) {
    await this.hide();
    await this.show(sel);
  }
}

customElements.define("quiz-views", ViewsElement);

// This once inherited from HTMLButtonElement,
// but it turns out that doesn't work on WebKit
class PlayStopButton extends HTMLElement {
  #audio;
  #playing;
  #internals;
  constructor() {
    super();
    // god i hate webdev
    if (ElementInternals !== undefined) {
      this.#internals = this.attachInternals();
      this.#internals.role = "button";
    }
  }

  connectedCallback() {
    this.#audio = document.createElement("audio");
    this.#playing = false;
    this.tabIndex = 0;

    this.#audio.addEventListener("play", this.#playStopCallback.bind(this, true));
    for (const ev of ["pause", "ended"])
      this.#audio.addEventListener(ev, this.#playStopCallback.bind(this, false));

    this.#audio.addEventListener("timeupdate", () => {
      if (this.maxDuration !== undefined && this.currentTime > this.maxDuration)
        this.stop();
    });

    this.addEventListener("click", () => this.#togglePlaying());
    this.addEventListener("keydown", ev => {
      if (ev.key == "Enter") this.#togglePlaying();
    });
  }

  #togglePlaying() {
    if (this.#playing) this.stop();
    else this.play();
  }

  #playStopCallback(playing) {
    this.replaceChildren(playing ? "Stop audio" : "Play audio");
    this.#playing = playing;
  }

  play() {
    this.#audio.currentTime = 0;
    this.#audio.play();
  }

  stop() {
    this.#audio.pause();
  }

  get src() { return this.#audio.src; }
  set src(newsrc) { this.#audio.src = newsrc; }

  get currentTime() { return this.#audio.currentTime; }

  get volume() { return this.#audio.volume; }
  set volume(newvol) { this.#audio.volume = newvol; }
}

customElements.define("quiz-play-stop-btn", PlayStopButton);

class QuizElement extends HTMLElement {
  #score;
  #total;
  #songno;
  #qaSection;
  #playButton;
  #quizOptions;
  #qAnsCover;
  #qAnsTitle;
  #qAnsAlbum;
  #qAnsBtns;

  constructor() {
    super();
  }

  async connectedCallback() {
    await wait_event(window, "load");
    this.#qaSection = this.querySelector("#quiz-qa-section");
    this.#playButton = this.querySelector("#play-button");
    this.#playButton.maxDuration = 10;
    this.#quizOptions = this.querySelector("#quiz-options");
    this.#qAnsCover = this.querySelector("#quiz-q-ans-cover");
    this.#qAnsTitle = this.querySelector("#quiz-q-ans-title");
    this.#qAnsAlbum = this.querySelector("#quiz-q-ans-album");
    this.#qAnsBtns = this.querySelector("#quiz-q-ans-btns");

    this.reset(0);
  }

  reset(newtotal) {
    this.score = 0;
    this.total = newtotal;
    this.songno = 0;
  }

  async ask(preview_url, options) {
    this.#playButton.src = preview_url;
    this.#playButton.play();

    const answer_promises = [];
    this.#quizOptions.replaceChildren();

    for (const option of options) {
      const answer_btn = document.createElement("button");
      answer_btn.replaceChildren(option);
      answer_promises.push(wait_event(answer_btn, "click").then(() => option));
      this.#quizOptions.appendChild(answer_btn);
    }

    await this.#qaSection.show("#quiz-question");

    const answer = await Promise.race(answer_promises);

    await this.#qaSection.hide();
    this.#playButton.stop();
    return answer;
  }

  async showAnswer(correct, { album_title, album_cover_url, title }, options) {
    this.#qAnsCover.src = album_cover_url;
    this.#qAnsTitle.replaceChildren(title);
    this.#qAnsAlbum.replaceChildren(album_title);
    this.#qAnsBtns.replaceChildren();

    const optionPromises = [];
    for (const { label, value } of options) {
      const button = document.createElement("button");
      button.replaceChildren(label);
      this.#qAnsBtns.appendChild(button);
      optionPromises.push(wait_event(button, "click").then(() => value));
    }

    await wait_event(this.#qAnsCover, "load");
    await this.#qaSection.show("#quiz-q-answer");
    const retval = await Promise.race(optionPromises);
    await this.#qaSection.hide();

    return retval;
  }

  get score() {
    return this.#score;
  }

  set score(newscore) {
    this.#score = newscore;
    this.querySelector("#quiz-score").replaceChildren(this.#score.toString())
  }

  get total() {
    return this.#total;
  }

  set total(newtotal) {
    this.#total = newtotal;
    this.querySelector("#quiz-total").replaceChildren(this.#total.toString());
  }


  get songno() {
    return this.#songno;
  }

  set songno(newsongno) {
    this.#songno = newsongno;
    this.querySelector("#quiz-songno").replaceChildren(this.#songno.toString());
  }
}

customElements.define("quiz-elem", QuizElement);

async function show_results(score, guessed, total) {
  const scorestr = (score == total) ? "PERFECT" : `${score}/${guessed}`;
  document.querySelector("#quiz-final-score").replaceChildren(scorestr);

  await document.querySelector("#top-level-views").switch("#quiz-results");
  await wait_event(document.querySelector("#quiz-try-again-btn"), "click");
}

async function run_quiz() {
  const qs = await questions;
  const quiz = document.querySelector("#quiz");
  const toplevel_views = document.querySelector("#top-level-views");

  await toplevel_views.hide();
  quiz.reset(qs.length);
  await toplevel_views.show("#quiz");

  for (const q of qs) {
    quiz.songno++;
    let user_answer = await quiz.ask(q.answer_info.preview_url, q.options);
    const correct = q.answer_info.title === user_answer;
    if (correct) quiz.score++;
    const answerOptions = quiz.songno !== quiz.total
            ? [ {label: "Next song",   value: false}
              , {label: "Finish quiz", value: true} ]
            : [ {label: "Finish quiz", value: true} ]
    if (await quiz.showAnswer(correct, q.answer_info, answerOptions)) break;
  }

  await show_results(quiz.score, quiz.songno, quiz.total);
  questions = get_questions();
  return run_quiz();
}

async function start() {
  await wait_event(window, "load");
  const start_button = document.querySelector("#quiz-start-button");
  const toplevel_views = document.querySelector("#top-level-views");

  for (;;) {
    await wait_event(start_button, "click");

    try {
      await run_quiz();
    } catch (e) {
      alert(e);
    }

    await toplevel_views.switch("#artist-info");
  }
}

start();
