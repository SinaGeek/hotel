from django.shortcuts import render, get_object_or_404, redirect
from django.contrib.auth.decorators import login_required
from .models import Guest, GuestDocument
from apps.reservations.models import Reservation, ReservationGuest
from django.db.models import Sum

@login_required
def guest_list_view(request):
    guests = Guest.objects.all().order_by('last_name', 'first_name')
    
    guest_data = []
    for g in guests:
        # Calculate stays and payments
        # Stays are reservations where the guest is either the main guest or a companion
        res_ids = ReservationGuest.objects.filter(guest=g).values_list('reservation_id', flat=True)
        stays = Reservation.objects.filter(id__in=res_ids)
        
        total_paid = stays.aggregate(total=Sum('paid_amount'))['total'] or 0
        total_nights = 0
        for s in stays:
            if s.check_out_date and s.check_in_date:
                total_nights += (s.check_out_date - s.check_in_date).days
        
        guest_data.append({
            'guest': g,
            'total_stays': stays.count(),
            'total_paid': total_paid,
            'total_nights': total_nights
        })
        
    return render(request, 'guests/guest_list.html', {'guests': guest_data})

@login_required
def guest_detail_view(request, guest_id):
    guest = get_object_or_404(Guest, id=guest_id)
    
    if request.method == 'POST':
        if 'id_photo' in request.FILES:
            guest.id_photo = request.FILES['id_photo']
            guest.save()
            return redirect('guest_detail', guest_id=guest.id)
            
        # Update other info if needed
        guest.first_name = request.POST.get('first_name', guest.first_name)
        guest.last_name = request.POST.get('last_name', guest.last_name)
        guest.email = request.POST.get('email', guest.email)
        guest.phone = request.POST.get('phone', guest.phone)
        guest.national_id = request.POST.get('national_id', guest.national_id)
        guest.passport_number = request.POST.get('passport_number', guest.passport_number)
        guest.notes = request.POST.get('notes', guest.notes)
        guest.save()
        return redirect('guest_detail', guest_id=guest.id)

    # Get stay history
    res_guests = ReservationGuest.objects.filter(guest=guest).select_related('reservation', 'reservation__room')
    
    history = []
    for rg in res_guests:
        res = rg.reservation
        # Who stayed with them?
        companions = ReservationGuest.objects.filter(reservation=res).exclude(guest=guest).select_related('guest')
        
        history.append({
            'reservation': res,
            'is_primary': rg.is_primary,
            'companions': companions
        })
        
    return render(request, 'guests/guest_detail.html', {
        'guest': guest,
        'history': history
    })
