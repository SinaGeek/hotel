from django.shortcuts import render, redirect
from django.contrib.auth import get_user_model, authenticate, login, logout
from django.core.mail import send_mail
from django.urls import reverse
from django.conf import settings
from django.http import HttpResponse
from django.contrib.auth.decorators import login_required
from django.db.models import Q

User = get_user_model()

@login_required
def logout_view(request):
    logout(request)
    return redirect('login')

@login_required
def profile_view(request):
    from .models import ActivityLog
    if request.method == 'POST':
        request.user.first_name = request.POST.get('first_name')
        request.user.last_name = request.POST.get('last_name')
        password = request.POST.get('password')
        if password:
            request.user.set_password(password)
        request.user.save()
        
        # Log the activity
        ActivityLog.objects.create(user=request.user, action="بروزرسانی اطلاعات کاربری", is_visible_to_user=True)
        
        return render(request, 'accounts/profile.html', {
            'message': 'اطلاعات با موفقیت ذخیره شد.',
            'logs': ActivityLog.objects.filter(user=request.user, is_visible_to_user=True)
        })
    
    context = {
        'logs': ActivityLog.objects.filter(user=request.user, is_visible_to_user=True)
    }
    return render(request, 'accounts/profile.html', context)

def login_view(request):
    if request.method == 'POST':
        login_id = request.POST.get('email') # This field can be email or username
        password = request.POST.get('password')
        
        # Check if user exists by email or username
        user_exists = User.objects.filter(Q(email=login_id) | Q(username=login_id)).exists()
        
        if not user_exists:
            # User doesn't exist, redirect to register with email prepopulated if it looks like an email
            return redirect(f"{reverse('register')}?email={login_id}")
            
        # Try authenticating with email as username first
        user = authenticate(request, username=login_id, password=password)
        
        # If that fails, try finding the user by email and then authenticating with their username
        if user is None:
            user_obj = User.objects.filter(email=login_id).first()
            if user_obj:
                user = authenticate(request, username=user_obj.username, password=password)

        if user is not None:
            login(request, user)
            from .models import ActivityLog
            ActivityLog.objects.create(user=user, action="ورود به سیستم")
            if user.is_staff:
                return redirect('/admin/')
            return redirect('/')
        else:
            return render(request, 'accounts/login.html', {'error': 'رمز عبور یا شناسه اشتباه است یا اکانت شما هنوز فعال نشده است.'})
    return render(request, 'accounts/login.html')

def register_view(request):
    if request.method == 'POST':
        email = request.POST.get('email')
        password = request.POST.get('password')
        first_name = request.POST.get('first_name')
        last_name = request.POST.get('last_name')
        
        user_qs = User.objects.filter(email=email)
        if user_qs.exists():
            if not user_qs.first().is_active:
                return render(request, 'accounts/register.html', {'error': 'درخواست قبلی شما در دست بررسی است مجددا تلاش نکنید و منتظر پیام ادمین باشید.'})
            return render(request, 'accounts/register.html', {'error': 'این ایمیل قبلا ثبت و فعال شده است.'})
            
        user = User.objects.create_user(
            username=email, email=email, password=password,
            first_name=first_name, last_name=last_name, is_active=False
        )
        
        activation_link = request.build_absolute_uri(reverse('activate_user', args=[user.id]))
        rejection_link = request.build_absolute_uri(reverse('reject_user', args=[user.id]))
        send_mail(
            'درخواست عضویت جدید',
            f'کاربر جدیدی ({email}) ثبت نام کرده است.\n\nبرای تایید: {activation_link}\nبرای رد درخواست: {rejection_link}',
            settings.DEFAULT_FROM_EMAIL,
            [admin_email for admin_name, admin_email in settings.ADMINS],
            fail_silently=False,
        )
        return render(request, 'accounts/registration_success.html')
    initial_email = request.GET.get('email', '')
    return render(request, 'accounts/register.html', {'initial_email': initial_email})

def activate_user_view(request, user_id):
    if not request.user.is_superuser:
        return HttpResponse("Unauthorized.", status=403)
        
    from .models import Role
    if request.method == 'POST':
        role_id = request.POST.get('role_id')
        if not role_id:
            return HttpResponse("انتخاب نقش الزامی است.", status=400)
            
        try:
            user = User.objects.get(id=user_id)
            role = Role.objects.get(id=role_id)
            user.role = role
            user.is_active = True
            user.save()
            
            from .models import ActivityLog
            ActivityLog.objects.create(user=user, action=f"تایید عضویت با نقش {role.name}", is_visible_to_user=True)
            
            # Redirect back to admin user list
            return HttpResponse(f'''
                <script>
                    alert("کاربر {user.email} با نقش {role.name} فعال شد.");
                    window.location.href = "/admin/accounts/user/";
                </script>
            ''')
        except Exception as e:
            return HttpResponse(f"Error: {e}", status=400)
            
    try:
        user = User.objects.get(id=user_id)
        roles = Role.objects.all()
        
        html = f'''
        <html dir="rtl"><body style="font-family:Tahoma; padding:20px;">
        <h3>تایید کاربر: {user.email}</h3>
        <form method="post">
            <input type="hidden" name="csrfmiddlewaretoken" value="{request.COOKIES.get('csrftoken', '')}">
            <label>انتخاب نقش (Role):</label>
            <select name="role_id" required>
                <option value="">-- انتخاب کنید --</option>
                {"".join([f'<option value="{r.id}">{r.name}</option>' for r in roles])}
            </select><br><br>
            <button type="submit">تایید و فعال‌سازی</button>
        </form>
        </body></html>
        '''
        return HttpResponse(html)
    except User.DoesNotExist:
        return HttpResponse("User not found.", status=404)

def reject_user_view(request, user_id):
    if not request.user.is_superuser:
        return HttpResponse("Unauthorized.", status=403)
        
    try:
        user = User.objects.get(id=user_id)
        if not user.is_active:
            user_email = user.email
            user.delete()
            # Send rejection email to user
            send_mail(
                'رد درخواست عضویت',
                'درخواست عضویت شما رد شد. لطفا دوباره تلاش کنید.',
                settings.DEFAULT_FROM_EMAIL,
                [user_email],
                fail_silently=False,
            )
            return HttpResponse(f"کاربر {user_email} رد و حذف شد.")
        return HttpResponse("این کاربر قبلا فعال شده است.", status=400)
    except User.DoesNotExist:
        return HttpResponse("User not found.", status=404)
