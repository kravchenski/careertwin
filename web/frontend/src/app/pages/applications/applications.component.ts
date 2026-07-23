import { AfterViewInit, Component } from "@angular/core";
import { CommonModule } from "@angular/common";
import gsap from "gsap";

@Component({
  selector: "ct-applications",
  standalone: true,
  imports: [CommonModule],
  template: `
    <div class="applications page-shell">
      <section class="applications__hero surface">
        <div>
          <h1 class="section-title">Follow-up pressure is lower when the process is visible.</h1>
        </div>
      </section>

      <section class="applications__pipeline">
        <article class="stage surface" *ngFor="let stage of pipeline">
          <div class="stage__top">
            <span>{{ stage.name }}</span>
            <strong>{{ stage.count }}</strong>
          </div>
          <div class="stage__bar">
            <div class="stage__fill" [style.width.%]="stage.percentage" [style.background]="stage.color"></div>
          </div>
        </article>
      </section>

      <section class="applications__list">
        <article class="application surface" *ngFor="let app of applications">
          <div class="application__head">
            <div class="application__identity">
              <div class="application__logo">
                <img
                  [src]="'https://api.companyenrich.com/logo/' + app.domain"
                  [alt]="app.company + ' logo'"
                  (error)="onLogoError($event)"
                  loading="lazy"
                />
              </div>
              <div>
                <h2>{{ app.position }}</h2>
                <p>{{ app.company }}</p>
              </div>
            </div>
            <span class="application__status" [class]="'application__status--' + app.status">{{ app.statusLabel }}</span>
          </div>

          <div class="application__timeline">
            <div
              class="timeline-step"
              *ngFor="let event of app.timeline; let last = last"
              [class.timeline-step--completed]="event.completed"
              [class.timeline-step--active]="event.active"
            >
              <div class="timeline-step__dot"></div>
              <strong>{{ event.label }}</strong>
              <span *ngIf="event.date">{{ event.date }}</span>
              <div class="timeline-step__line" *ngIf="!last"></div>
            </div>
          </div>

          <div class="application__notes">
            <div class="note" *ngFor="let note of app.notes">
              <strong>{{ note.content }}</strong>
              <span>{{ note.date }}</span>
            </div>
          </div>
        </article>
      </section>
    </div>
  `,
  styles: [`
    .applications {
      display: grid;
      gap: 1rem;
      padding-bottom: 4rem;
    }

    .surface {
      border: 1px solid rgba(209, 161, 87, 0.12);
      border-radius: 1.5rem;
      background: rgba(255, 255, 255, 0.03);
      transition: border-color 160ms, background 160ms;
    }

    .stage.surface,
    .application.surface {
      border-radius: 1.2rem;
    }

    .stage.surface:hover,
    .application.surface:hover {
      border-color: rgba(209, 161, 87, 0.25);
      background: rgba(255, 255, 255, 0.045);
    }

    .applications__hero,
    .stage,
    .application {
      padding: 1.3rem;
    }

    .applications__pipeline {
      display: grid;
      grid-template-columns: repeat(4, minmax(0, 1fr));
      gap: 1rem;
    }

    .stage__top,
    .application__head,
    .application__identity,
    .note {
      display: flex;
      justify-content: space-between;
      gap: 1rem;
    }

    .stage__top {
      margin-bottom: 0.8rem;
      color: #baa690;
    }

    .stage__top strong {
      color: #f4e5cd;
      font-family: var(--font-display);
      font-size: 1.45rem;
    }

    .stage__bar {
      height: 0.42rem;
      background: rgba(255, 255, 255, 0.06);
      border-radius: 999px;
      overflow: hidden;
    }

    .stage__fill {
      height: 100%;
      border-radius: inherit;
    }

    .applications__list {
      display: grid;
      gap: 1rem;
    }

    .application__identity {
      align-items: center;
    }

    .application__logo {
      width: 3rem;
      height: 3rem;
      display: inline-flex;
      align-items: center;
      justify-content: center;
      border-radius: 0.8rem;
      background: rgba(255, 255, 255, 0.06);
      border: 1px solid rgba(209, 161, 87, 0.15);
      font-weight: 700;
      overflow: hidden;
    }

    .application__logo img {
      width: 100%;
      height: 100%;
      object-fit: contain;
    }

    .application h2 {
      font-family: var(--font-sans);
      font-weight: 600;
      font-size: 1.1rem;
      color: #f4e5cd;
      margin-bottom: 0.25rem;
    }

    .application p {
      color: #baa690;
    }

    .application__status {
      height: fit-content;
      padding: 0.48rem 0.8rem;
      border-radius: 999px;
      border: 1px solid rgba(209, 161, 87, 0.15);
      background: rgba(255, 255, 255, 0.04);
      font-size: 0.85rem;
      font-weight: 600;
    }

    .application__status--offer {
      color: #34d399;
    }

    .application__status--technical {
      color: #fbbf24;
    }

    .application__status--phone-screen {
      color: #d1a157;
    }

    .application__status--applied {
      color: #c4b49e;
    }

    .application__timeline {
      display: grid;
      grid-template-columns: repeat(4, minmax(0, 1fr));
      gap: 0.75rem;
      margin: 1rem 0;
      padding: 1rem;
      border-radius: 1.25rem;
      background: rgba(255, 255, 255, 0.03);
      border: 1px solid rgba(209, 161, 87, 0.12);
    }

    .timeline-step {
      position: relative;
      display: grid;
      gap: 0.35rem;
      min-width: 0;
    }

    .timeline-step strong {
      font-size: 0.92rem;
      color: #f4e5cd;
    }

    .timeline-step span {
      color: #baa690;
      font-size: 0.8rem;
    }

    .timeline-step__dot {
      width: 0.85rem;
      height: 0.85rem;
      border-radius: 999px;
      background: rgba(255, 255, 255, 0.18);
      margin-bottom: 0.2rem;
      z-index: 1;
      transition: background 200ms;
    }

    .timeline-step__line {
      position: absolute;
      top: 0.42rem;
      left: 1rem;
      right: -0.85rem;
      height: 1px;
      background: rgba(255, 255, 255, 0.12);
    }

    .timeline-step--completed .timeline-step__dot,
    .timeline-step--active .timeline-step__dot {
      background: #d1a157;
    }

    .timeline-step--completed .timeline-step__line {
      background: rgba(209, 161, 87, 0.5);
    }

    .timeline-step--active .timeline-step__dot {
      box-shadow: 0 0 0 5px rgba(209, 161, 87, 0.18);
    }

    .application__notes {
      display: grid;
      gap: 0.7rem;
    }

    .note {
      align-items: center;
      padding: 0.85rem 0.95rem;
      border-radius: 1rem;
      background: rgba(255, 255, 255, 0.03);
      border: 1px solid rgba(209, 161, 87, 0.12);
    }

    .note span {
      color: #baa690;
      white-space: nowrap;
    }

    @media (max-width: 980px) {
      .applications__pipeline,
      .application__timeline {
        grid-template-columns: repeat(2, minmax(0, 1fr));
      }
    }

    @media (max-width: 720px) {
      .applications__pipeline,
      .application__timeline {
        grid-template-columns: 1fr;
      }

      .application__head,
      .application__identity,
      .note {
        flex-direction: column;
        align-items: flex-start;
      }
    }
  `],
})
export class ApplicationsComponent implements AfterViewInit {
  readonly pipeline = [
    { name: "Applied", count: 12, percentage: 100, color: "rgba(255,255,255,0.55)" },
    { name: "Phone screen", count: 6, percentage: 74, color: "#7dd3fc" },
    { name: "Technical", count: 3, percentage: 42, color: "#fcd34d" },
    { name: "Offer", count: 1, percentage: 16, color: "#86efac" },
  ];

  readonly applications = [
    {
      company: "Figma",
      position: "Design systems engineer",
      status: "phone-screen",
      statusLabel: "Phone screen",
      domain: "figma.com",
      timeline: [
        { label: "Applied", date: "Jul 01", completed: true, active: false },
        { label: "Recruiter chat", date: "Jul 04", completed: true, active: false },
        { label: "Hiring manager", date: "Jul 08", completed: false, active: true },
        { label: "Panel", date: "", completed: false, active: false },
      ],
      notes: [
        { content: "Highlight systems adoption metrics, not just component count.", date: "Last edited 2h ago" },
        { content: "Strong signal: recruiter reacted well to collaboration examples.", date: "Follow-up due tomorrow" },
      ],
    },
    {
      company: "Stripe",
      position: "Staff frontend engineer",
      status: "technical",
      statusLabel: "Technical loop",
      domain: "stripe.com",
      timeline: [
        { label: "Applied", date: "Jun 27", completed: true, active: false },
        { label: "Recruiter chat", date: "Jun 30", completed: true, active: false },
        { label: "Technical", date: "Jul 10", completed: false, active: true },
        { label: "Offer", date: "", completed: false, active: false },
      ],
      notes: [
        { content: "Prepare examples around developer productivity and platform governance.", date: "Prep block 45 min" },
        { content: "Resume version B tested better than generic variant.", date: "ATS match 91%" },
      ],
    },
  ];

  ngAfterViewInit() {
    gsap.from(".applications .surface", {
      y: 24,
      opacity: 0,
      duration: 0.55,
      stagger: 0.08,
      ease: "power3.out",
    });
  }

  onLogoError(event: Event) {
    const img = event.target as HTMLImageElement;
    img.style.display = "none";
  }
}
