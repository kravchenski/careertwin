import { AfterViewInit, Component, ElementRef, ViewChild, OnDestroy } from "@angular/core";
import { CommonModule } from "@angular/common";
import { RouterLink } from "@angular/router";
import gsap from "gsap";
import { ScrollTrigger } from "gsap/ScrollTrigger";

gsap.registerPlugin(ScrollTrigger);

@Component({
  selector: "ct-home",
  standalone: true,
  imports: [CommonModule, RouterLink],
  template: `
    <div class="home-root">
      <section class="hero-scene">
        <div class="hero-glow" aria-hidden="true"></div>
        <div class="hero-grain" aria-hidden="true"></div>
        <div class="hero-copy">
          <div class="hero-text">
            <div class="hero-line display-outline">The career tool that</div>
            <div class="hero-line">makes boring resumes</div>
            <div class="hero-line hero-line--accent">look weak</div>
          </div>
          <p class="hero-sub">Build a Knowledge Graph of your professional DNA. <br class="hide-mobile">Generate tailored, ATS-optimized documents for every opportunity.</p>
          <div class="hero-cta">
            <a routerLink="/studio" class="btn-primary">
              <span>Open Studio</span>
              <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M5 12h14M13 5l7 7-7 7"/></svg>
            </a>
            <a routerLink="/resume" class="btn-ghost">
              <span>See a sample CV</span>
            </a>
          </div>
        </div>
      </section>

      <section class="bento-scene">
        <div class="bento-head">
          <h2 class="bento-title">Everything a recruiter's screen checks<br class="hide-mobile"> before a human ever does.</h2>
        </div>
        <div class="bento-grid">
          <a routerLink="/resume" class="bento-card bento-card--wide">
            <div class="bento-card__glow"></div>
            <h3>ATS optimization</h3>
            <p>Every draft is checked against the same keyword and formatting rules an applicant tracking system runs first, before a person ever opens it.</p>
          </a>
          <a routerLink="/resume" class="bento-card">
            <div class="bento-card__glow"></div>
            <h3>Visual QA</h3>
            <p>Vision AI inspects the rendered page for spacing, overflow, and contrast, then repairs what it finds.</p>
          </a>
          <a routerLink="/jobs" class="bento-card">
            <div class="bento-card__glow"></div>
            <h3>Multi-language</h3>
            <p>Applications generate in the language the posting was written in, automatically.</p>
          </a>
          <a routerLink="/studio" class="bento-card bento-card--wide">
            <div class="bento-card__glow"></div>
            <h3>Knowledge Graph</h3>
            <p>One structured graph of your experience powers every tailored document: GitHub, projects, and work history connected once, reused everywhere.</p>
          </a>
        </div>
      </section>

      <section class="steps-wrapper" #stepsWrapper>
        <div class="steps-track">
          <div class="step-panel" data-step="1">
            <div class="step-panel__number">01</div>
            <div class="step-panel__content">
              <div class="step-tag">Collect</div>
              <p>Connect GitHub and your website. We build a Knowledge Graph of everything you've ever done professionally.</p>
            </div>
          </div>
          <div class="step-panel" data-step="2">
            <div class="step-panel__number">02</div>
            <div class="step-panel__content">
              <div class="step-tag">Tailor</div>
              <p>For each job, AI analyzes relevance, ranks your experience, and assembles the strongest version of your resume.</p>
            </div>
          </div>
          <div class="step-panel" data-step="3">
            <div class="step-panel__number">03</div>
            <div class="step-panel__content">
              <div class="step-tag">Polish</div>
              <p>Vision AI inspects layout, spacing, and readability. Auto-repairs issues until your resume passes every check.</p>
            </div>
          </div>
        </div>
      </section>

      <section class="final-cta">
        <div class="final-cta__glow" aria-hidden="true"></div>
        <h2 class="final-cta__title">Stop guessing what<br>gets a callback.</h2>
        <a routerLink="/studio" class="btn-primary btn-primary--large">
          <span>Open Studio</span>
          <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M5 12h14M13 5l7 7-7 7"/></svg>
        </a>
      </section>

      <footer class="site-footer">
        <div class="footer-inner">
          <div class="footer-top">
            <div class="footer-brand">
              <span class="footer-logo">JobTwin</span>
              <p class="footer-tagline">Your AI-powered career operating system.</p>
            </div>
            <div class="footer-links">
              <div class="footer-col">
                <h4>Product</h4>
                <a routerLink="/profile">Profile</a>
                <a routerLink="/resume">Resume builder</a>
              </div>
              <div class="footer-col">
                <h4>Features</h4>
                <a routerLink="/studio">Knowledge Graph</a>
                <a routerLink="/resume">ATS optimization</a>
                <a routerLink="/resume">Visual QA</a>
                <a routerLink="/jobs">Multi-language</a>
              </div>
              <div class="footer-col">
                <h4>Resources</h4>
                <a href="https://jobtwin.app/docs" target="_blank" rel="noopener">Documentation</a>
                <a href="https://jobtwin.app/api" target="_blank" rel="noopener">API reference</a>
                <a href="https://jobtwin.app/changelog" target="_blank" rel="noopener">Changelog</a>
                <a href="https://jobtwin.app/support" target="_blank" rel="noopener">Support</a>
              </div>
            </div>
          </div>
          <div class="footer-bottom">
            <span>&copy; 2026 JobTwin. All rights reserved.</span>
            <div class="footer-bottom-links">
              <a href="https://jobtwin.app/privacy" target="_blank">Privacy</a>
              <a href="https://jobtwin.app/terms" target="_blank">Terms</a>
            </div>
          </div>
        </div>
      </footer>
    </div>
  `,
  styles: [`
    :host {
      display: block;
    }

    .home-root {
      padding-bottom: 0;
      overflow-x: hidden;
      max-width: 100%;
      position: relative;
      background:
        radial-gradient(60rem 40rem at 15% 0%, rgba(200, 155, 80, 0.07), transparent 55%),
        radial-gradient(50rem 60rem at 100% 40%, rgba(200, 155, 80, 0.05), transparent 60%),
        radial-gradient(46rem 50rem at 0% 85%, rgba(200, 155, 80, 0.06), transparent 55%);
    }

    /* ── Hero ─────────────────────────────────────────────────── */

    .hero-scene {
      min-height: 88dvh;
      display: flex;
      align-items: center;
      justify-content: center;
      padding: 6rem 2rem 4rem;
      position: relative;
      overflow: hidden;
    }

    .hero-scene::after {
      content: "";
      position: absolute;
      bottom: 0;
      left: 10%;
      right: 10%;
      height: 1px;
      background: linear-gradient(90deg, transparent, var(--color-line-strong), transparent);
    }

    .hero-glow {
      position: absolute;
      inset: 0;
      z-index: 0;
      pointer-events: none;
      background:
        radial-gradient(46rem 30rem at 50% 8%, rgba(200, 155, 80, 0.16), transparent 60%),
        radial-gradient(38rem 26rem at 12% 92%, rgba(200, 155, 80, 0.08), transparent 65%);
    }

    .hero-grain {
      position: fixed;
      inset: 0;
      z-index: 0;
      pointer-events: none;
      opacity: 0.045;
      mix-blend-mode: overlay;
      background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='120' height='120'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='2' stitchTiles='stitch'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E");
    }

    .hero-copy {
      text-align: center;
      max-width: min(68rem, 88vw);
      position: relative;
      z-index: 1;
    }

    .hero-text {
      display: flex;
      flex-direction: column;
      align-items: center;
    }

    .hero-line {
      font-family: var(--font-display);
      font-size: clamp(4.2rem, 6.4vw, 9.5rem);
      line-height: 0.9;
      letter-spacing: 0.01em;
      text-wrap: balance;
      color: var(--color-foreground);
    }

    .hero-line--accent {
      color: var(--color-accent);
    }

    .hero-sub {
      margin-top: 2.4rem;
      font-size: clamp(1rem, 1.6vw, 1.2rem);
      line-height: 1.7;
      color: var(--color-muted);
      max-width: 40ch;
      margin-left: auto;
      margin-right: auto;
    }

    .hide-mobile {
      display: block;
    }

    .hero-cta {
      margin-top: 2.8rem;
      display: flex;
      align-items: center;
      justify-content: center;
      gap: 1.5rem;
      flex-wrap: wrap;
    }

    .btn-ghost {
      display: inline-flex;
      align-items: center;
      gap: 0.5rem;
      color: var(--color-muted);
      font-size: 0.95rem;
      text-decoration: none;
      border-bottom: 1px solid transparent;
      transition: color 0.25s ease, border-color 0.25s ease;
      padding-bottom: 0.15rem;
    }

    .btn-ghost:hover {
      color: var(--color-foreground);
      border-color: var(--color-line-strong);
    }

    /* ── Bento features ───────────────────────────────────────── */

    .bento-scene {
      max-width: 76rem;
      margin: 0 auto;
      padding: 8rem 2rem;
    }

    .bento-head {
      max-width: 44rem;
      margin: 0 0 3.5rem;
    }

    .bento-title {
      font-family: var(--font-display);
      font-size: clamp(2.2rem, 4vw, 3.4rem);
      line-height: 1.05;
      letter-spacing: 0.005em;
      color: var(--color-foreground);
    }

    .bento-grid {
      display: grid;
      grid-template-columns: repeat(3, 1fr);
      grid-auto-flow: dense;
      gap: 1.25rem;
    }

    .bento-card {
      position: relative;
      display: block;
      grid-column: span 1;
      padding: 2.4rem 2rem;
      border-radius: 1.4rem;
      border: 1px solid var(--color-line);
      background: var(--color-panel);
      text-decoration: none;
      overflow: hidden;
      transition: transform 0.5s cubic-bezier(0.16, 1, 0.3, 1), border-color 0.4s ease, background 0.4s ease;
    }

    .bento-card--wide {
      grid-column: span 2;
    }

    .bento-card:hover {
      transform: translateY(-6px);
      border-color: var(--color-line-strong);
      background: var(--color-panel-strong);
    }

    .bento-card__glow {
      position: absolute;
      inset: -40% -40% auto auto;
      width: 14rem;
      height: 14rem;
      background: radial-gradient(circle, rgba(200, 155, 80, 0.22), transparent 70%);
      opacity: 0;
      transition: opacity 0.5s ease;
      pointer-events: none;
    }

    .bento-card:hover .bento-card__glow {
      opacity: 1;
    }

    .bento-card h3 {
      font-family: var(--font-logo);
      font-size: 1.3rem;
      font-weight: 600;
      color: var(--color-foreground);
      margin-bottom: 0.7rem;
      position: relative;
      z-index: 1;
    }

    .bento-card p {
      font-size: 0.92rem;
      line-height: 1.65;
      color: var(--color-muted);
      position: relative;
      z-index: 1;
      max-width: 34rem;
    }

    /* ── Steps ────────────────────────────────────────────────── */

    .steps-wrapper {
      position: relative;
      height: 250vh;
    }

    .steps-track {
      position: sticky;
      top: 0;
      display: flex;
      width: 300vw;
      height: 100dvh;
      overflow: hidden;
    }

    .step-panel {
      width: 100vw;
      height: 100dvh;
      position: relative;
      display: flex;
      align-items: center;
      justify-content: center;
      flex-shrink: 0;
    }

    .step-panel__number {
      position: absolute;
      left: 50%;
      top: 50%;
      transform: translate(-50%, -50%);
      font-family: var(--font-display);
      font-size: clamp(16rem, 30vw, 34rem);
      line-height: 0.82;
      color: rgba(200, 155, 80, 0.04);
      pointer-events: none;
      z-index: 0;
      user-select: none;
    }

    .step-panel__content {
      position: relative;
      z-index: 1;
      text-align: center;
      max-width: 34rem;
      padding: 0 2rem;
    }

    .step-panel__content p {
      font-size: clamp(1.05rem, 1.6vw, 1.35rem);
      line-height: 1.75;
      letter-spacing: -0.005em;
      margin-top: 1.4rem;
      color: var(--color-muted);
    }

    .step-tag {
      display: inline-block;
      font-family: var(--font-logo);
      font-size: 0.82rem;
      font-weight: 600;
      letter-spacing: 0.2em;
      text-transform: uppercase;
      color: var(--color-accent);
      padding-bottom: 0.5rem;
      border-bottom: 1px solid rgba(200, 155, 80, 0.3);
    }

    /* ── Final CTA ────────────────────────────────────────────── */

    .final-cta {
      position: relative;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 2.6rem;
      padding: 10rem 2rem;
      text-align: center;
      overflow: hidden;
    }

    .final-cta__glow {
      position: absolute;
      inset: 0;
      z-index: 0;
      pointer-events: none;
      background: radial-gradient(40rem 24rem at 50% 50%, rgba(200, 155, 80, 0.14), transparent 65%);
    }

    .final-cta__title {
      position: relative;
      z-index: 1;
      font-family: var(--font-display);
      font-size: clamp(2.6rem, 6vw, 5.2rem);
      line-height: 1;
      color: var(--color-foreground);
    }

    .btn-primary--large {
      position: relative;
      z-index: 1;
      font-size: 1.05rem;
      padding: 1.1rem 2.2rem;
    }

    /* ── Footer ───────────────────────────────────────────────── */

    .site-footer {
      border-top: 1px solid var(--color-line);
      padding: 5rem 0 2.5rem;
      background: linear-gradient(180deg, var(--color-background) 0%, #080604 100%);
    }

    .footer-inner {
      max-width: 72rem;
      margin: 0 auto;
      padding: 0 2rem;
    }

    .footer-top {
      display: grid;
      grid-template-columns: 1.2fr 2fr;
      gap: 4rem;
      margin-bottom: 4rem;
    }

    .footer-logo {
      font-family: var(--font-logo);
      font-size: 1.3rem;
      font-weight: 700;
      color: var(--color-foreground);
      letter-spacing: -0.02em;
    }

    .footer-tagline {
      color: var(--color-muted);
      font-size: 0.85rem;
      margin-top: 0.6rem;
      max-width: 18rem;
      line-height: 1.6;
    }

    .footer-links {
      display: grid;
      grid-template-columns: repeat(3, 1fr);
      gap: 2rem;
    }

    .footer-col h4 {
      font-family: var(--font-logo);
      font-size: 0.7rem;
      font-weight: 600;
      text-transform: uppercase;
      letter-spacing: 0.14em;
      color: var(--color-foreground);
      margin-bottom: 1.2rem;
      opacity: 0.7;
    }

    .footer-col a {
      display: block;
      color: var(--color-muted);
      font-size: 0.85rem;
      text-decoration: none;
      padding: 0.35rem 0;
      transition: color 0.2s, transform 0.2s;
    }

    .footer-col a:hover {
      color: var(--color-foreground);
      transform: translateX(3px);
    }

    .footer-bottom {
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding-top: 2rem;
      border-top: 1px solid var(--color-line);
      color: var(--color-muted);
      font-size: 0.78rem;
    }

    .footer-bottom-links {
      display: flex;
      gap: 1.5rem;
    }

    .footer-bottom-links a {
      color: var(--color-muted);
      text-decoration: none;
      transition: color 0.2s;
    }

    .footer-bottom-links a:hover {
      color: var(--color-foreground);
    }

    /* ── Mobile ───────────────────────────────────────────────── */

    @media (max-width: 768px) {
      .hero-scene {
        min-height: 70dvh;
        padding: 5rem 1.5rem 3rem;
      }

      .hero-line {
        font-size: clamp(3.2rem, 12vw, 5rem);
        line-height: 0.95;
      }

      .hide-mobile {
        display: none;
      }

      .hero-sub {
        font-size: 0.95rem;
        margin-top: 1.8rem;
      }

      .bento-scene {
        padding: 5rem 1.5rem;
      }

      .bento-grid {
        grid-template-columns: 1fr;
      }

      .bento-card--wide {
        grid-column: span 1;
      }

      .steps-wrapper {
        height: 400vh;
      }

      .steps-track {
        flex-direction: column;
        width: 100vw;
        height: 100dvh;
        overflow: visible;
      }

      .step-panel {
        width: 100vw;
        min-height: 100dvh;
        padding: 4rem 1.5rem;
      }

      .step-panel__content {
        padding: 0 1rem;
      }

      .step-panel__content p {
        font-size: 1rem;
      }

      .final-cta {
        padding: 6rem 1.5rem;
      }

      .footer-top {
        grid-template-columns: 1fr;
        gap: 2.5rem;
      }

      .footer-links {
        grid-template-columns: repeat(2, 1fr);
      }

      .footer-bottom {
        flex-direction: column;
        gap: 1rem;
        text-align: center;
      }
    }

    @media (max-width: 480px) {
      .hero-scene {
        min-height: 60dvh;
        padding: 4rem 1.25rem 2.5rem;
      }

      .hero-line {
        font-size: clamp(2.8rem, 13vw, 4rem);
      }

      .step-panel {
        padding-top: 5rem;
      }
    }
  `],
})
export class HomeComponent implements AfterViewInit, OnDestroy {
  @ViewChild("stepsWrapper") stepsWrapperRef!: ElementRef<HTMLElement>;
  private animationFrameId: number | null = null;

  ngAfterViewInit() {
    this.initAnimations();
    setTimeout(() => ScrollTrigger.refresh(), 100);
  }

  ngOnDestroy() {
    gsap.killTweensOf("*");
    ScrollTrigger.getAll().forEach((st) => st.kill());
    if (this.animationFrameId) {
      cancelAnimationFrame(this.animationFrameId);
    }
  }

  private initAnimations() {
    const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (reduceMotion) return;

    gsap.from(".hero-line", {
      yPercent: 110,
      opacity: 0,
      duration: 1,
      stagger: 0.1,
      ease: "power3.out",
    });

    gsap.from(".hero-sub", {
      y: 20,
      opacity: 0,
      duration: 0.8,
      delay: 0.4,
      ease: "power3.out",
    });

    gsap.from(".hero-cta", {
      y: 16,
      opacity: 0,
      duration: 0.7,
      delay: 0.6,
      ease: "power3.out",
    });

    gsap.to(".hero-glow", {
      opacity: 0.6,
      duration: 4,
      ease: "sine.inOut",
      yoyo: true,
      repeat: -1,
    });

    gsap.from(".bento-title", {
      y: 24,
      opacity: 0,
      duration: 0.9,
      ease: "power3.out",
      scrollTrigger: {
        trigger: ".bento-head",
        start: "top 85%",
      },
    });

    gsap.utils.toArray<HTMLElement>(".bento-card").forEach((card, i) => {
      gsap.from(card, {
        y: 40,
        opacity: 0,
        duration: 0.8,
        delay: i * 0.08,
        ease: "power3.out",
        scrollTrigger: {
          trigger: card,
          start: "top 88%",
        },
      });
    });

    const stepsTrack = document.querySelector(".steps-track") as HTMLElement;
    const stepsWrapper = document.querySelector(".steps-wrapper") as HTMLElement;
    if (stepsTrack && stepsWrapper) {
      const isMobile = window.innerWidth <= 768;

      if (isMobile) {
        const totalHeight = stepsTrack.scrollHeight - window.innerHeight;
        gsap.to(stepsTrack, {
          y: -totalHeight,
          ease: "none",
          scrollTrigger: {
            trigger: ".steps-wrapper",
            start: "top top",
            end: "bottom bottom",
            scrub: 1.5,
            snap: 1 / (document.querySelectorAll(".step-panel").length - 1),
            invalidateOnRefresh: true,
          },
        });
      } else {
        const totalWidth = stepsTrack.scrollWidth - window.innerWidth;
        const panelCount = document.querySelectorAll(".step-panel").length;

        gsap.to(stepsTrack, {
          x: -totalWidth,
          ease: "none",
          scrollTrigger: {
            trigger: ".steps-wrapper",
            start: "top top",
            end: "bottom bottom",
            scrub: 1.5,
            snap: 1 / (panelCount - 1),
            invalidateOnRefresh: true,
          },
        });
      }

      document.querySelectorAll<HTMLElement>(".step-panel__number").forEach((num) => {
        gsap.fromTo(num,
          { yPercent: 60, opacity: 0 },
          {
            yPercent: 0,
            opacity: 1,
            duration: 1.2,
            ease: "power2.out",
            scrollTrigger: {
              trigger: num.closest(".step-panel"),
              start: isMobile ? "top 75%" : "left 75%",
              end: isMobile ? "top 25%" : "left 25%",
              scrub: 1,
            },
          }
        );
      });

      document.querySelectorAll<HTMLElement>(".step-panel__content").forEach((content) => {
        gsap.fromTo(content,
          { y: 30, opacity: 0 },
          {
            y: 0,
            opacity: 1,
            duration: 0.9,
            ease: "power3.out",
            scrollTrigger: {
              trigger: content.closest(".step-panel"),
              start: isMobile ? "top 70%" : "left 70%",
              end: isMobile ? "top 30%" : "left 30%",
              scrub: 1,
            },
          }
        );
      });
    }

    gsap.from(".final-cta__title", {
      y: 30,
      opacity: 0,
      duration: 1,
      ease: "power3.out",
      scrollTrigger: {
        trigger: ".final-cta",
        start: "top 75%",
      },
    });

    gsap.from(".final-cta .btn-primary", {
      y: 16,
      opacity: 0,
      duration: 0.8,
      delay: 0.2,
      ease: "power3.out",
      scrollTrigger: {
        trigger: ".final-cta",
        start: "top 75%",
      },
    });
  }
}
