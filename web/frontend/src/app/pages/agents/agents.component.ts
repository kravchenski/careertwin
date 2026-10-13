import { Component, OnDestroy, OnInit, inject } from "@angular/core";
import { CommonModule } from "@angular/common";
import { FormsModule } from "@angular/forms";
import { firstValueFrom } from "rxjs";
import { AgentDetail, AgentJob, AgentSettings, AgentStatus, AutomationService } from "../../services/automation.service";

@Component({
  selector: "ct-agents", standalone: true, imports: [CommonModule, FormsModule],
  template: `
    <div class="page-shell agents">
      <header class="surface hero">
        <div><span class="eyebrow">Your job search</span><h1 class="section-title">Four agents. One application history.</h1>
          <p>Find relevant roles, prepare a CV from your experience, apply and keep every answer.</p></div>
        <a class="btn-secondary" [href]="api.exportUrl">Download Excel</a>
      </header>
      <p class="notice error" role="alert" *ngIf="error">{{ error }}</p>
      <p class="notice error" role="alert" *ngIf="status?.excel_error">Excel could not be updated: {{ status?.excel_error }}</p>
      <p class="notice" role="status" *ngIf="notice">{{ notice }}</p>
      <section class="stages" aria-label="Workflow stages">
        <article class="surface"><span>01 · Find & rank</span><strong>{{ count(['priority','stretch','needs_info','skip']) }}</strong><p>Match to your documented experience.</p></article>
        <article class="surface"><span>02 · Prepare CV</span><strong>{{ count(['prepared','form_ready']) }}</strong><p>Relevant evidence, readable PDF.</p></article>
        <article class="surface"><span>03 · Apply</span><strong>{{ count(['submitted','interview','offer','hired']) }}</strong><p>Confirmed applications, {{ status?.submitted_today || 0 }} attempts today.</p></article>
        <article class="surface"><span>04 · Keep history</span><strong>{{ jobs.length }}</strong><p>Documents, answers and outcomes.</p></article>
      </section>

      <section class="surface controls" *ngIf="settings">
        <div><h2>Search controls</h2><p>{{ status?.running ? 'Searching and processing vacancies…' : 'Ready for the next search' }}</p></div>
        <div class="buttons"><button class="btn-primary" [disabled]="busy || status?.running || !status?.decision_configured" (click)="start()">Start search</button>
          <button class="btn-secondary" [disabled]="busy || (!status?.running && !status?.active_jobs?.length && !settings.scheduled)" (click)="stop()">Stop</button>
          <button class="btn-secondary" (click)="showSettings = !showSettings">{{ showSettings ? 'Close settings' : 'Edit settings' }}</button></div>
        <p class="notice" *ngIf="!status?.decision_configured">Add your TypeSafe API key to use Jev, or select Switchyard cloud models in settings.</p>
      </section>

      <form class="surface settings" *ngIf="showSettings && settings" (ngSubmit)="save()">
        <h2>Your context and search rules</h2>
        <p>All agents use the same ABOUT.md. <a href="/studio">Edit your answers in Studio</a> · <a href="/api/v1/studio/about" download="ABOUT.md">Download ABOUT.md</a>. Salary: Auto, adapted to each vacancy unless you select a fixed amount in Studio.</p><label>Shared candidate context<textarea name="context" rows="10" [ngModel]="settings.context" readonly placeholder="Complete Studio to generate ABOUT.md."></textarea></label>
        <div class="columns"><label>Target roles from Studio<textarea name="queries" rows="4" [(ngModel)]="queries" [readOnly]="settings.context.includes('CAREERTWIN_STUDIO_STATE')"></textarea></label>
          <label>Work preferences from Studio<textarea name="preferences" rows="4" [(ngModel)]="settings.preferences" [readOnly]="settings.context.includes('CAREERTWIN_STUDIO_STATE')"></textarea></label></div>
        <fieldset><legend>Job sources</legend><div class="source-grid"><label class="check" *ngFor="let source of sources">
          <input type="checkbox" [checked]="settings.sources.includes(source.id)" (change)="toggleSource(source.id)">{{ source.domain }}
        </label></div><p>Remote OK and Remotive use public feeds. Other boards use search results or imported links.</p></fieldset>
        <fieldset><legend>Company career boards</legend>
          <div class="company" *ngFor="let company of settings.companies; let i = index">
            <input [name]="'company'+i" [(ngModel)]="company['name']" placeholder="Company board identifier">
            <input [name]="'career'+i" [(ngModel)]="company['url']" placeholder="Career page URL">
            <button type="button" class="btn-secondary" (click)="settings.companies.splice(i,1)">Remove</button>
          </div><button type="button" class="btn-secondary" (click)="settings.companies.push({name:'',url:''})">Add company</button>
        </fieldset>
        <div class="columns small"><label>Minimum fit score<input type="number" name="threshold" min="0" max="100" [(ngModel)]="settings.threshold"></label>
          <label>Vacancies per cycle<input type="number" name="limit" min="1" max="100" [(ngModel)]="settings.limit"></label>
          <label>Maximum daily submissions<input type="number" name="daily" min="1" max="100" [(ngModel)]="settings.daily_limit"></label>
          <label>Search interval, hours<input type="number" name="interval" min="1" max="168" [(ngModel)]="settings.interval_hours"></label></div>
        <div class="columns"><label>Text model provider<input name="provider" [(ngModel)]="settings.provider"></label>
          <label>Text model<input name="model" [(ngModel)]="settings.model"></label></div>
        <div class="columns"><label>Decision service<select name="decisionBackend" [(ngModel)]="settings.decision_backend">
          <option value="switchyard">Switchyard cloud models</option><option value="typesafe">TypeSafe Jev</option></select></label>
          <label *ngIf="settings.decision_backend === 'switchyard'">Decision model<input name="decisionModel" list="decision-models" [(ngModel)]="settings.decision_model" placeholder="auto or a model ID from Switchyard">
            <datalist id="decision-models"><option *ngFor="let model of decisionModels" [value]="model.id"></option></datalist>
            <button type="button" class="btn-secondary" (click)="loadModels()">Load available models</button></label>
          <label *ngIf="settings.decision_backend === 'typesafe'">Jev model<input name="jevModel" [(ngModel)]="settings.jev_model"></label></div>
        <p>Switchyard uses connected cloud models. Their certainty estimates are approximate and have not been calibrated on your hiring outcomes.</p>
        <label class="check"><input type="checkbox" name="prepare" [(ngModel)]="settings.auto_prepare">Prepare CVs automatically for priority roles</label>
        <label class="check"><input type="checkbox" name="submit" [(ngModel)]="settings.auto_submit">Submit priority applications automatically when all answers are known</label>
        <label class="check"><input type="checkbox" name="scheduled" [(ngModel)]="settings.scheduled">Repeat searches while CareerTwin is running</label>
        <button class="btn-primary" [disabled]="busy">Save settings</button>
      </form>

      <details class="surface import"><summary>Import a vacancy</summary><form (ngSubmit)="importJob()">
        <div class="columns"><label>Job URL<input name="url" required type="url" [(ngModel)]="draft.url"></label>
          <label>Role<input name="title" required [(ngModel)]="draft.title"></label><label>Company<input name="company" [(ngModel)]="draft.company"></label></div>
        <label>Full description (optional if available on the page)<textarea name="description" rows="4" [(ngModel)]="draft.description"></textarea></label>
        <button class="btn-primary" [disabled]="busy">Add vacancy</button></form></details>

      <section class="surface results"><div class="list-head"><h2>Vacancies & applications</h2><span>{{ jobs.length }} records</span></div>
        <p *ngIf="!jobs.length && !loading">Start a search or import a vacancy. Your applications and answers will appear here.</p>
        <p *ngIf="loading">Loading your workflow…</p>
        <article class="job" *ngFor="let job of jobs">
          <div class="job-main"><a [href]="job.vacancy.url" target="_blank" rel="noopener noreferrer"><h3>{{ job.vacancy.title }}</h3></a>
            <p>{{ job.vacancy.company || job.vacancy.source }} · {{ job.vacancy.location || 'Location to verify' }}</p>
            <p class="gaps" *ngIf="job.evaluation?.gaps?.length">Gaps: {{ job.evaluation?.gaps?.join(', ') }}</p>
            <p class="error" *ngIf="job.error">{{ job.error }}</p></div>
          <div class="job-rank"><strong>{{ job.evaluation ? job.evaluation.score + '/100' : 'Unranked' }}</strong><span>{{ label(job.state) }}</span></div>
          <div class="buttons"><button class="btn-secondary" (click)="inspect(job)">Details</button>
            <button class="btn-secondary" [disabled]="active(job)" (click)="action(job,'rank')" *ngIf="!terminal(job)">Rank</button>
            <button class="btn-secondary" [disabled]="active(job)" (click)="action(job,'prepare')" *ngIf="job.evaluation && !terminal(job)">Prepare CV</button>
            <button class="btn-secondary" [disabled]="active(job)" (click)="action(job,'fill')" *ngIf="job.package && !terminal(job) && job.state !== 'submission_uncertain'">Fill form</button>
            <button class="btn-primary" [disabled]="active(job)" (click)="action(job,'submit')" *ngIf="job.package && !terminal(job) && job.state !== 'submission_uncertain'">Submit application</button>
            <button class="btn-secondary" [disabled]="active(job)" (click)="action(job,'verify')" *ngIf="job.state === 'submission_uncertain'">Check confirmation</button>
          </div>
        </article>
      </section>

      <section class="surface detail" *ngIf="detail">
        <div class="list-head"><h2>{{ detail.job.vacancy.company }} · {{ detail.job.vacancy.title }}</h2><button class="btn-secondary" (click)="detail = null">Close</button></div>
        <p>{{ label(detail.job.state) }} <span *ngIf="detail.job.evaluation">· Fit {{ detail.job.evaluation.score }}/100 · {{ detail.job.evaluation.confidence_method === 'model_estimate_uncalibrated' ? 'Estimated certainty' : 'Confidence' }} {{ detail.job.evaluation.confidence | percent:'1.0-0' }} · {{ detail.job.evaluation.model }}</span></p>
        <p>Fit measures documented compatibility, not your probability of receiving an offer.</p>
        <a class="btn-secondary" *ngIf="detail.job.package?.cv_url" [href]="'/api' + detail.job.package?.cv_url" target="_blank">Open CV</a>
        <form *ngFor="let question of pendingQuestions()" (ngSubmit)="answer(question)"><label>{{ question }}<input [name]="question" [(ngModel)]="answerDraft[question]" required></label><button class="btn-secondary">Save answer</button></form>
        <button class="btn-primary" [disabled]="active(detail.job)" *ngIf="detail.job.state === 'awaiting_answers'" (click)="action(detail.job,'resume')">Continue application</button>
        <div class="buttons outcomes"><span>Update outcome:</span><button class="btn-secondary" *ngFor="let outcome of outcomes" (click)="setOutcome(outcome)">{{ label(outcome) }}</button></div>
        <h3>Documents</h3><p class="file" *ngFor="let file of detail.files"><strong>{{ file.kind }}</strong> {{ file.path }}<small>SHA256 {{ file.sha256 }}</small></p>
        <h3>History & exact answers</h3><details class="event" *ngFor="let event of detail.events"><summary>{{ event.created_at | date:'MMM d, HH:mm:ss' }} · {{ event.stage }}</summary><pre>{{ event.payload | json }}</pre></details>
      </section>
    </div>
  `,
  styles: [`
    .agents{display:grid;gap:1.2rem}.hero,.controls,.list-head{display:flex;justify-content:space-between;align-items:center;gap:1rem}.hero,.controls,.settings,.import,.results,.detail{padding:1.5rem}.hero p,p{color:var(--color-muted);line-height:1.5}.hero h1{max-width:850px}.stages{display:grid;grid-template-columns:repeat(4,1fr);gap:1rem}.stages article{padding:1.2rem}.stages span{display:block;color:var(--color-muted)}.stages strong{display:block;font-size:2rem;margin-top:.7rem}.stages p{font-size:.85rem;margin-bottom:0}.buttons{display:flex;flex-wrap:wrap;gap:.6rem;align-items:center}h2{font-size:1.2rem;margin:0 0 .6rem}h3{font-size:1rem;margin:0 0 .3rem}.notice{padding:1rem;border:1px solid var(--color-line);border-radius:1rem}.error{color:#f0a09a!important}.controls{flex-wrap:wrap}.controls .notice{width:100%}form,label{display:grid;gap:.6rem}.settings{gap:1.2rem}.columns{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:1rem}.small{grid-template-columns:repeat(4,minmax(0,1fr))}input,textarea,select{width:100%;background:var(--color-panel-strong);color:var(--color-foreground);border:1px solid var(--color-line);border-radius:.65rem;padding:.8rem;font:inherit;box-sizing:border-box}textarea{resize:vertical}.check{display:flex;align-items:center;gap:.6rem}.check input{width:auto}fieldset{border:1px solid var(--color-line);padding:1rem;border-radius:.8rem}.source-grid{display:grid;grid-template-columns:repeat(3,1fr);gap:.6rem}.source-grid label{font-size:.85rem}.company{display:flex;gap:.6rem;margin-bottom:.7rem}.import summary,.event summary{cursor:pointer}.import form{margin-top:1rem}.job{padding:1.3rem 0;border-top:1px solid var(--color-line);display:grid;grid-template-columns:1fr auto;gap:.7rem}.job .buttons{grid-column:1/-1}.job a{text-decoration:none;color:inherit}.job-rank{display:grid;align-content:start;gap:.4rem;text-align:right}.job-rank span{font-size:.8rem;color:var(--color-muted)}.gaps{font-size:.85rem;color:#edc47d}.event{padding:.8rem 0;border-top:1px solid var(--color-line)}pre{white-space:pre-wrap;overflow-wrap:anywhere;font-size:.8rem;max-height:450px;overflow:auto}.detail form{margin:1rem 0}.outcomes{margin:1.3rem 0}.file{overflow-wrap:anywhere;font-size:.85rem}.file small{display:block;font-size:.7rem}button:disabled{opacity:.5;cursor:wait}@media(max-width:800px){.stages{grid-template-columns:repeat(2,1fr)}.hero{align-items:flex-start;flex-direction:column}.columns,.small{grid-template-columns:1fr}.source-grid{grid-template-columns:repeat(2,1fr)}.company{flex-wrap:wrap}}@media(max-width:440px){.source-grid{grid-template-columns:1fr}}
  `],
})
export class AgentsComponent implements OnInit, OnDestroy {
  api = inject(AutomationService);
  settings: AgentSettings | null = null;
  sources: {id:string;domain:string;method:string}[] = [];
  decisionModels: {id:string}[] = [];
  jobs: AgentJob[] = [];
  status: AgentStatus | null = null;
  detail: AgentDetail | null = null;
  showSettings = false; loading = true; busy = false;
  error = ""; notice = ""; queries = "";
  answerDraft: Record<string,string> = {};
  draft = {url:"",title:"",company:"",description:""};
  outcomes = ["interview","offer","hired","rejected","withdrawn"];
  private timer?: ReturnType<typeof setInterval>;
  private refreshing = false;
  async ngOnInit() {
    try { this.settings = await firstValueFrom(this.api.settings()); this.queries = this.settings.queries.join("\n"); this.sources = await firstValueFrom(this.api.sources()); this.showSettings = !this.settings.context; }
    catch (e) { this.fail(e); }
    await this.refresh(); this.timer = setInterval(() => void this.refresh(),5000);
  }
  ngOnDestroy() { if(this.timer) clearInterval(this.timer); }
  async refresh() {
    if(this.refreshing) return; this.refreshing=true;
    try { [this.jobs,this.status] = await Promise.all([firstValueFrom(this.api.jobs()),firstValueFrom(this.api.status())]); if(this.detail) this.detail=await firstValueFrom(this.api.detail(this.detail.job.id)); }
    catch(e) { this.fail(e); } finally { this.loading=false; this.refreshing=false; }
  }
  private fail(e: unknown) { const x=e as {error?:{detail?:unknown};message?:string}; this.error=typeof x.error?.detail === 'string' ? x.error.detail : x.message || 'Request failed'; }
  private async perform(fn:()=>Promise<unknown>,message:string) { this.error='';this.busy=true;try{await fn();this.notice=message;await this.refresh();}catch(e){this.fail(e);}finally{this.busy=false;} }
  async save() { if(!this.settings)return;this.settings.queries=this.queries.split('\n').map(x=>x.trim()).filter(Boolean);await this.perform(()=>firstValueFrom(this.api.save(this.settings!)),'Settings saved.'); }
  async start() { await this.perform(()=>firstValueFrom(this.api.run()),'Search started. Progress is saved as each agent finishes.'); }
  async stop() { await this.perform(()=>firstValueFrom(this.api.stop()),'Stopping the workflow and scheduled searches.'); }
  async loadModels() { await this.perform(async()=>{this.decisionModels=await firstValueFrom(this.api.decisionModels());},'Available cloud models loaded.'); }
  async importJob() { await this.perform(()=>firstValueFrom(this.api.import(this.draft)),'Vacancy added.');this.draft={url:'',title:'',company:'',description:''}; }
  async action(job:AgentJob,operation:string) { await this.perform(()=>firstValueFrom(this.api.action(job.id,operation)),'Agent started.'); }
  async inspect(job:AgentJob) { try{this.detail=await firstValueFrom(this.api.detail(job.id));}catch(e){this.fail(e);} }
  async answer(question:string) { if(!this.detail)return;await this.perform(()=>firstValueFrom(this.api.answer(this.detail!.job.id,question,this.answerDraft[question])),'Answer saved.'); }
  async setOutcome(status:string) { if(!this.detail)return;await this.perform(()=>firstValueFrom(this.api.outcome(this.detail!.job.id,status)),'Outcome recorded.'); }
  toggleSource(id:string) { if(!this.settings)return;const i=this.settings.sources.indexOf(id);if(i<0)this.settings.sources.push(id);else this.settings.sources.splice(i,1); }
  pendingQuestions():string[] { const event=this.detail?.events.filter(x=>x.stage==='questions').at(-1);return this.detail?.job.state==='awaiting_answers' && Array.isArray(event?.payload['questions']) ? event.payload['questions'] as string[] : []; }
  active(job:AgentJob) { return this.status?.active_jobs.includes(job.id) || false; }
  terminal(job:AgentJob) { return ['submitted',...this.outcomes].includes(job.state); }
  count(states:string[]) { return this.jobs.filter(x=>states.includes(x.state)).length; }
  label(value:string) { return ({priority:'Priority',stretch:'Stretch',needs_info:'Needs information',skip:'Poor fit',awaiting_answers:'Waiting for your answers',needs_review:'Needs review',form_ready:'Form ready',submission_uncertain:'Submission to verify'} as Record<string,string>)[value] || value.replaceAll('_',' ').replace(/^./,x=>x.toUpperCase()); }
}
