import { AfterViewInit, Component, OnInit, inject } from "@angular/core";
import { CommonModule } from "@angular/common";
import { FormsModule } from "@angular/forms";
import gsap from "gsap";
import { Application, ApplicationStage, ApplicationsService } from "../../services/applications.service";

const NEXT_STAGE: Record<string, string> = {
  applied: "phone_screen",
  phone_screen: "technical",
  technical: "offer",
};

@Component({
  selector: "ct-applications",
  standalone: true,
  imports: [CommonModule, FormsModule],
  template: `
    <div class="applications page-shell">
      <section class="applications__hero surface">
        <div>
          <h1 class="section-title">Follow-up pressure is lower when the process is visible.</h1>
        </div>
      </section>

      <section class="applications__pipeline" *ngIf="applications.length">
        <article class="stage surface" *ngFor="let stage of pipeline">
          <div class="stage__top">
            <span>{{ stage.name }}</span>
            <strong>{{ stage.count }}</strong>
          </div>
          <div class="stage__bar">
            <div class="stage__fill" [style.width.%]="stage.percentage" [style.background]="stageColor(stage.name)"></div>
          </div>
        </article>
      </section>

      <p class="applications__empty surface" *ngIf="!loading && !applications.length">
        No applications yet. Once you apply to a job and confirm it was submitted, it will show up here.
      </p>

      <section class="applications__list">
        <article class="application surface" *ngFor="let app of applications">
          <div class="application__head">
            <div class="application__identity">
              <div class="application__logo">
                <img
                  *ngIf="app.domain"
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
            <span class="application__status" [class]="'application__status--' + app.status">{{ app.status_label }}</span>
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
              <span *ngIf="event.date">{{ event.date | date: "MMM d" }}</span>
              <div class="timeline-step__line" *ngIf="!last"></div>
            </div>
          </div>

          <div class="application__notes" *ngIf="app.notes.length">
            <div class="note" *ngFor="let note of app.notes">
              <strong>{{ note.content }}</strong>
              <span>{{ note.date | date: "MMM d, HH:mm" }}</span>
            </div>
          </div>

          <div class="application__actions">
            <button
              class="app-action-btn"
              type="button"
              *ngIf="nextStage(app.status)"
              (click)="advance(app)"
            >Mark as {{ stageLabel(nextStage(app.status)!) }}</button>
            <button
              class="app-action-btn app-action-btn--danger"
              type="button"
              *ngIf="app.status !== 'rejected'"
              (click)="reject(app)"
            >Mark as rejected</button>
            <input
              class="app-note-input"
              type="text"
              placeholder="Add a note..."
              [(ngModel)]="noteDraft[app.id]"
              (keyup.enter)="addNote(app)"
            />
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

    .applications__empty {
      padding: 1.6rem;
      color: #baa690;
      text-align: center;
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

    .application__status--phone_screen {
      color: #d1a157;
    }

    .application__status--applied {
      color: #c4b49e;
    }

    .application__status--rejected {
      color: #e57373;
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
      margin-bottom: 1rem;
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

    .application__actions {
      display: flex;
      flex-wrap: wrap;
      gap: 0.6rem;
      align-items: center;
    }

    .app-action-btn {
      border: 1px solid rgba(209, 161, 87, 0.25);
      border-radius: 999px;
      background: rgba(209, 161, 87, 0.1);
      color: #f5df90;
      padding: 0.5rem 1rem;
      font-weight: 600;
      font-size: 0.85rem;
      cursor: pointer;
    }
    .app-action-btn:hover { background: rgba(209, 161, 87, 0.18); }
    .app-action-btn--danger { color: #e57373; border-color: rgba(229, 115, 115, 0.25); background: rgba(229, 115, 115, 0.08); }
    .app-action-btn--danger:hover { background: rgba(229, 115, 115, 0.15); }

    .app-note-input {
      flex: 1;
      min-width: 12rem;
      border: 1px solid rgba(209, 161, 87, 0.2);
      border-radius: 999px;
      background: rgba(255, 255, 255, 0.05);
      color: #f4e5cd;
      padding: 0.5rem 1rem;
      font: inherit;
      font-size: 0.85rem;
      outline: none;
    }
    .app-note-input:focus { border-color: #d1a157; }

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
export class ApplicationsComponent implements OnInit, AfterViewInit {
  private readonly applicationsService = inject(ApplicationsService);

  applications: Application[] = [];
  pipeline: ApplicationStage[] = [];
  loading = true;
  noteDraft: Record<string, string> = {};

  private readonly stageColors: Record<string, string> = {
    Applied: "rgba(255,255,255,0.55)",
    "Phone screen": "#7dd3fc",
    Technical: "#fcd34d",
    Offer: "#86efac",
  };

  ngOnInit(): void {
    this.applicationsService.list().subscribe((res) => {
      this.applications = res.applications || [];
      this.pipeline = res.pipeline || [];
      this.loading = false;
    });
  }

  ngAfterViewInit() {
    gsap.from(".applications .surface", {
      y: 24,
      opacity: 0,
      duration: 0.55,
      stagger: 0.08,
      ease: "power3.out",
    });
  }

  stageColor(name: string): string {
    return this.stageColors[name] || "#d1a157";
  }

  nextStage(status: string): string | null {
    return NEXT_STAGE[status] || null;
  }

  stageLabel(status: string): string {
    const map: Record<string, string> = {
      phone_screen: "Phone screen",
      technical: "Technical",
      offer: "Offer",
    };
    return map[status] || status;
  }

  advance(app: Application): void {
    const next = this.nextStage(app.status);
    if (!next) return;
    this.applicationsService.update(app.id, { status: next }).subscribe((res) => {
      if (res.success && res.application) {
        this.applications = this.applications.map((a) => (a.id === app.id ? res.application! : a));
      }
    });
  }

  reject(app: Application): void {
    this.applicationsService.update(app.id, { status: "rejected" }).subscribe((res) => {
      if (res.success && res.application) {
        this.applications = this.applications.map((a) => (a.id === app.id ? res.application! : a));
      }
    });
  }

  addNote(app: Application): void {
    const note = (this.noteDraft[app.id] || "").trim();
    if (!note) return;
    this.applicationsService.update(app.id, { note }).subscribe((res) => {
      if (res.success && res.application) {
        this.applications = this.applications.map((a) => (a.id === app.id ? res.application! : a));
        this.noteDraft[app.id] = "";
      }
    });
  }

  onLogoError(event: Event) {
    const img = event.target as HTMLImageElement;
    img.style.display = "none";
  }
}
