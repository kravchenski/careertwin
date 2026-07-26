import { AfterViewInit, Component, ElementRef, ViewChild, OnDestroy, inject } from "@angular/core";
import { CommonModule } from "@angular/common";
import { RouterLink } from "@angular/router";
import gsap from "gsap";
import { ScrollTrigger } from "gsap/ScrollTrigger";
import { StudioService } from "../../services/studio.service";

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
              <span>{{ hasProfile() ? "Open Studio" : "Get Started" }}</span>
              <svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M5 12h14M13 5l7 7-7 7"/></svg>
            </a>
          </div>
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

      <footer class="site-footer">
        <div class="footer-inner">
          <div class="footer-top">
            <div class="footer-brand">
              <a routerLink="/" class="footer-mark">
                <span class="footer-mark-icon">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6">
                    <path d="M4 6.5C4 5.12 5.12 4 6.5 4H12a8 8 0 1 1 0 16H6.5A2.5 2.5 0 0 1 4 17.5z"/>
                    <path d="M8 9h6M8 12h8M8 15h5"/>
                  </svg>
                </span>
                <span class="footer-logo">JobTwin</span>
              </a>
              <p class="footer-tagline">Your AI-powered career operating system. It collects roles, tailors your materials, and prepares each application so you can spend your time on interviews.</p>
            </div>

            <nav class="footer-nav" aria-label="Product">
              <span class="footer-nav-title">Product</span>
              <a routerLink="/studio">Studio</a>
              <a routerLink="/jobs">Jobs</a>
              <a routerLink="/applications">Applications</a>
              <a routerLink="/dashboard">Dashboard</a>
            </nav>
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
        radial-gradient(46rem 34rem at 50% 34%, rgba(200, 155, 80, 0.16), transparent 62%),
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

    /* ── Steps ────────────────────────────────────────────────── */

    .steps-wrapper {
      position: relative;
      overflow: hidden;
    }

    .steps-track {
      display: flex;
      width: 300vw;
      height: 100dvh;
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
      display: flex;
      justify-content: space-between;
      align-items: flex-start;
      gap: 3rem;
      margin-bottom: 3.5rem;
    }

    .footer-brand {
      max-width: 24rem;
    }

    .footer-mark {
      display: inline-flex;
      align-items: center;
      gap: 0.6rem;
      text-decoration: none;
    }

    .footer-mark-icon {
      display: inline-flex;
      align-items: center;
      justify-content: center;
      width: 2.1rem;
      height: 2.1rem;
      border-radius: 999px;
      border: 1px solid var(--color-line-strong);
      background: var(--color-panel-strong);
      color: var(--color-foreground);
    }

    .footer-mark-icon svg {
      width: 1.05rem;
      height: 1.05rem;
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
      font-size: 0.88rem;
      margin-top: 0.9rem;
      line-height: 1.65;
    }

    .footer-nav {
      display: flex;
      flex-direction: column;
      gap: 0.85rem;
      min-width: 8rem;
    }

    .footer-nav-title {
      color: var(--color-foreground);
      font-size: 0.78rem;
      font-weight: 600;
    }

    .footer-nav a {
      color: var(--color-muted);
      text-decoration: none;
      font-size: 0.85rem;
      transition: color 0.2s;
    }

    .footer-nav a:hover {
      color: var(--color-foreground);
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

      .steps-track {
        flex-direction: column;
        width: 100vw;
        height: auto;
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

      .footer-top {
        flex-direction: column;
        gap: 2rem;
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
  private readonly studio = inject(StudioService);

  hasProfile(): boolean {
    return !!this.studio.load()?.completedAt;
  }

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

    const stepsTrack = document.querySelector(".steps-track") as HTMLElement;
    const stepsWrapper = document.querySelector(".steps-wrapper") as HTMLElement;
    if (stepsTrack && stepsWrapper) {
      const isMobile = window.innerWidth <= 768;
      const axis: "x" | "y" = isMobile ? "y" : "x";
      const panels = gsap.utils.toArray<HTMLElement>(".step-panel");
      const panelCount = panels.length;
      const getDistance = () =>
        isMobile
          ? stepsTrack.scrollHeight - window.innerHeight
          : stepsTrack.scrollWidth - window.innerWidth;

      const tl = gsap.timeline({
        defaults: { ease: "none" },
        scrollTrigger: {
          trigger: ".steps-wrapper",
          start: "top top",
          end: () => `+=${getDistance()}`,
          pin: true,
          scrub: 1,
          snap: 1 / (panelCount - 1),
          invalidateOnRefresh: true,
        },
      });

      tl.to(stepsTrack, { [axis]: () => -getDistance(), duration: panelCount - 1 }, 0);

      panels.forEach((panel, i) => {
        const num = panel.querySelector<HTMLElement>(".step-panel__number");
        const content = panel.querySelector<HTMLElement>(".step-panel__content");
        const center = i;

        if (num) {
          if (i > 0) {
            tl.fromTo(num,
              { opacity: 0, scale: 0.72 },
              { opacity: 1, scale: 1, duration: 0.45, ease: "power2.out" },
              center - 0.45
            );
          } else {
            gsap.set(num, { opacity: 1, scale: 1 });
          }
          if (i < panelCount - 1) {
            tl.to(num, { opacity: 0, scale: 1.22, duration: 0.45, ease: "power2.in" }, center + 0.05);
          }
        }

        if (content) {
          if (i > 0) {
            tl.fromTo(content,
              { opacity: 0, y: 56, scale: 0.94 },
              { opacity: 1, y: 0, scale: 1, duration: 0.45, ease: "power3.out" },
              center - 0.45
            );
          } else {
            gsap.set(content, { opacity: 1, y: 0, scale: 1 });
          }
          if (i < panelCount - 1) {
            tl.to(content, { opacity: 0, y: -44, scale: 0.94, duration: 0.4, ease: "power3.in" }, center + 0.08);
          }
        }
      });
    }
  }
}
